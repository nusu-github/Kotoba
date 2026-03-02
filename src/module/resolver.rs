use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use petgraph::algo::{kosaraju_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use tracing::{debug, instrument};

use crate::common::source::{SourceFile, Span};
use crate::diag::report::{Diagnostic, DiagnosticKind};
use crate::frontend::ast::{
    Block, ChainStep, Expr, ExprKind, FieldDef, LoopKind, MatchArm, Param, ParticleArg, Pattern,
    Program, Stmt, StmtKind, StringPart,
};
use crate::frontend::lexer::Lexer;
use crate::frontend::parser::Parser;
use crate::module::loader::{load_source, normalize_module_path};
use crate::sema::infer::analyze;

#[derive(Debug, Clone)]
pub struct ModuleDiagnostic {
    pub diagnostic: Diagnostic,
    pub source: Option<SourceFile>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProgram {
    pub root_source: SourceFile,
    pub program: Program,
    pub sources: Vec<MappedSource>,
}

#[derive(Debug, Clone)]
pub struct MappedSource {
    pub base: usize,
    pub source: SourceFile,
}

impl MappedSource {
    fn end(&self) -> usize {
        self.base + self.source.content.len()
    }
}

#[derive(Debug, Clone)]
struct ParsedModule {
    path: PathBuf,
    source: SourceFile,
    program: Program,
}

#[derive(Debug, Clone, Default)]
struct ImportPolicy {
    full: bool,
    items: BTreeSet<String>,
}

#[derive(Debug)]
struct GraphBuilder {
    modules: HashMap<PathBuf, ParsedModule>,
    policies: HashMap<PathBuf, ImportPolicy>,
    graph: DiGraph<PathBuf, ()>,
    nodes: IndexMap<PathBuf, NodeIndex>,
    loading: HashSet<PathBuf>,
    diagnostics: Vec<ModuleDiagnostic>,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
            policies: HashMap::new(),
            graph: DiGraph::new(),
            nodes: IndexMap::new(),
            loading: HashSet::new(),
            diagnostics: Vec::new(),
        }
    }

    fn ensure_node(&mut self, path: &PathBuf) -> NodeIndex {
        if let Some(idx) = self.nodes.get(path) {
            return *idx;
        }
        let idx = self.graph.add_node(path.clone());
        self.nodes.insert(path.clone(), idx);
        idx
    }

    fn add_dep_edge(&mut self, dependency: &PathBuf, importer: &PathBuf) {
        let dep_idx = self.ensure_node(dependency);
        let importer_idx = self.ensure_node(importer);
        if self.graph.find_edge(dep_idx, importer_idx).is_none() {
            self.graph.add_edge(dep_idx, importer_idx, ());
        }
    }

    fn visit(&mut self, path: PathBuf) {
        if self.modules.contains_key(&path) || self.loading.contains(&path) {
            return;
        }

        self.ensure_node(&path);
        self.loading.insert(path.clone());

        let source = match load_source(&path) {
            Ok(src) => src,
            Err(err) => {
                self.diagnostics.push(ModuleDiagnostic {
                    diagnostic: Diagnostic::new(
                        DiagnosticKind::Compile,
                        format!("モジュール読み込み失敗: {}", err),
                    )
                    .with_hint(format!("パス: {}", path.display())),
                    source: None,
                });
                self.loading.remove(&path);
                return;
            }
        };

        let (tokens, lex_errors) = Lexer::new(&source.content).tokenize();
        if !lex_errors.is_empty() {
            for err in lex_errors {
                self.diagnostics.push(ModuleDiagnostic {
                    diagnostic: Diagnostic::new(DiagnosticKind::Lex, err.message)
                        .with_span(err.span)
                        .with_hint(format!("モジュール: {}", path.display())),
                    source: Some(source.clone()),
                });
            }
            self.loading.remove(&path);
            return;
        }

        let (program, parse_errors) = Parser::new(tokens).parse();
        if !parse_errors.is_empty() {
            for err in parse_errors {
                self.diagnostics.push(ModuleDiagnostic {
                    diagnostic: Diagnostic::new(DiagnosticKind::Parse, err.message)
                        .with_span(err.span)
                        .with_hint(format!("モジュール: {}", path.display())),
                    source: Some(source.clone()),
                });
            }
            self.loading.remove(&path);
            return;
        }

        for diag in analyze(&program) {
            self.diagnostics.push(ModuleDiagnostic {
                diagnostic: diag,
                source: Some(source.clone()),
            });
        }

        let uses = collect_use_statements(&program);

        self.modules.insert(
            path.clone(),
            ParsedModule {
                path: path.clone(),
                source,
                program,
            },
        );

        for use_stmt in uses {
            let dep_raw = normalize_module_path(&path, &use_stmt.module);
            let dep = canonical_like(dep_raw);
            let policy = self.policies.entry(dep.clone()).or_default();
            if let Some(items) = use_stmt.items {
                for item in items {
                    policy.items.insert(item);
                }
            } else {
                policy.full = true;
            }

            self.add_dep_edge(&dep, &path);
            self.visit(dep);
        }

        self.loading.remove(&path);
    }
}

#[derive(Debug, Clone)]
struct UseStmt {
    module: String,
    items: Option<Vec<String>>,
}

#[instrument(skip_all, fields(root = %root.display()))]
pub fn resolve_root_program(root: &Path) -> Result<ResolvedProgram, Vec<ModuleDiagnostic>> {
    let root_path = canonical_like(root.to_path_buf());

    let mut builder = GraphBuilder::new();
    builder.visit(root_path.clone());

    if !builder.diagnostics.is_empty() {
        return Err(builder.diagnostics);
    }

    let mut cycle_diags = Vec::new();
    for component in kosaraju_scc(&builder.graph) {
        let is_cycle = if component.len() > 1 {
            true
        } else {
            let node = component[0];
            builder.graph.find_edge(node, node).is_some()
        };

        if !is_cycle {
            continue;
        }

        let cycle_nodes = recover_cycle_path(&builder.graph, &component).unwrap_or_else(|| {
            let mut fallback = component.clone();
            if let Some(first) = component.first() {
                fallback.push(*first);
            }
            fallback
        });

        let cycle_str = cycle_nodes
            .iter()
            .filter_map(|idx| builder.graph.node_weight(*idx))
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");

        cycle_diags.push(ModuleDiagnostic {
            diagnostic: Diagnostic::new(
                DiagnosticKind::Compile,
                format!("モジュール循環参照を検出しました: {cycle_str}"),
            )
            .with_hint("`使う` の依存関係を非循環（DAG）にしてください"),
            source: None,
        });
    }

    if !cycle_diags.is_empty() {
        return Err(cycle_diags);
    }

    let root_module = match builder.modules.get(&root_path) {
        Some(m) => m.clone(),
        None => {
            return Err(vec![ModuleDiagnostic {
                diagnostic: Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("ルートモジュールが見つかりません: {}", root_path.display()),
                ),
                source: None,
            }]);
        }
    };

    let ordered_nodes = match toposort(&builder.graph, None) {
        Ok(nodes) => nodes,
        Err(err) => {
            let node = builder
                .graph
                .node_weight(err.node_id())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            return Err(vec![ModuleDiagnostic {
                diagnostic: Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!("依存順序の確定に失敗しました: {node}"),
                )
                .with_hint("依存関係グラフを確認してください"),
                source: None,
            }]);
        }
    };

    debug!(
        module_count = builder.modules.len(),
        "module graph resolved"
    );

    let mut merged = Vec::new();
    let mut sources = vec![MappedSource {
        base: 0,
        source: root_module.source.clone(),
    }];
    let mut next_base = root_module.source.content.len().saturating_add(1);

    // dependency -> importer の向きで edge を張っているため、toposort 順で依存が先に来る。
    for node in ordered_nodes {
        let Some(module_path) = builder.graph.node_weight(node) else {
            continue;
        };
        if module_path == &root_path {
            continue;
        }

        let Some(module) = builder.modules.get(module_path) else {
            continue;
        };
        let policy = builder
            .policies
            .get(module_path)
            .cloned()
            .unwrap_or_default();

        let mut exported = exported_statements(module);
        let base = next_base;
        next_base = next_base
            .saturating_add(module.source.content.len())
            .saturating_add(1);
        sources.push(MappedSource {
            base,
            source: module.source.clone(),
        });
        if !policy.full {
            exported.retain(|stmt| {
                exported_name(stmt)
                    .map(|name| policy.items.contains(name))
                    .unwrap_or(false)
            });
        }

        if let Some(items) = (!policy.full).then_some(policy.items) {
            for item in items {
                let public_exists = exported_statements(module)
                    .into_iter()
                    .any(|stmt| exported_name(&stmt) == Some(item.as_str()));
                if !public_exists {
                    return Err(vec![ModuleDiagnostic {
                        diagnostic: Diagnostic::new(
                            DiagnosticKind::Compile,
                            format!(
                                "モジュール「{}」に公開項目「{}」がありません",
                                module.path.display(),
                                item
                            ),
                        )
                        .with_hint("`公開` 指定された手順・組・特性のみ import できます"),
                        source: Some(module.source.clone()),
                    }]);
                }
            }
        }

        merged.extend(exported.into_iter().map(|stmt| offset_stmt(&stmt, base)));
    }

    // ルートの use 文を除いた文を追加。
    for stmt in &root_module.program.statements {
        if !matches!(stmt.kind, StmtKind::Use { .. }) {
            merged.push(stmt.clone());
        }
    }

    let merged_program = Program {
        statements: merged,
        span: Span::new(
            0,
            sources
                .iter()
                .map(MappedSource::end)
                .max()
                .unwrap_or(root_module.source.content.len()),
        ),
    };

    Ok(ResolvedProgram {
        root_source: root_module.source,
        program: merged_program,
        sources,
    })
}

fn recover_cycle_path(
    graph: &DiGraph<PathBuf, ()>,
    component: &[NodeIndex],
) -> Option<Vec<NodeIndex>> {
    let allowed: HashSet<NodeIndex> = component.iter().copied().collect();

    for &start in component {
        let mut path = Vec::new();
        let mut on_stack = HashSet::new();
        if dfs_cycle(graph, &allowed, start, start, &mut path, &mut on_stack) {
            return Some(path);
        }
    }

    None
}

fn dfs_cycle(
    graph: &DiGraph<PathBuf, ()>,
    allowed: &HashSet<NodeIndex>,
    start: NodeIndex,
    current: NodeIndex,
    path: &mut Vec<NodeIndex>,
    on_stack: &mut HashSet<NodeIndex>,
) -> bool {
    path.push(current);
    on_stack.insert(current);

    for next in graph.neighbors(current) {
        if !allowed.contains(&next) {
            continue;
        }
        if next == start && path.len() > 1 {
            path.push(start);
            return true;
        }
        if !on_stack.contains(&next) && dfs_cycle(graph, allowed, start, next, path, on_stack) {
            return true;
        }
    }

    on_stack.remove(&current);
    path.pop();
    false
}

fn collect_use_statements(program: &Program) -> Vec<UseStmt> {
    program
        .statements
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::Use { module, items } => Some(UseStmt {
                module: module.clone(),
                items: items.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn exported_statements(module: &ParsedModule) -> Vec<Stmt> {
    module
        .program
        .statements
        .iter()
        .filter(|stmt| match &stmt.kind {
            StmtKind::ProcDef { is_public, .. }
            | StmtKind::StructDef { is_public, .. }
            | StmtKind::TraitDef { is_public, .. } => *is_public,
            _ => false,
        })
        .cloned()
        .collect()
}

fn exported_name(stmt: &Stmt) -> Option<&str> {
    match &stmt.kind {
        StmtKind::ProcDef { name, .. }
        | StmtKind::StructDef { name, .. }
        | StmtKind::TraitDef { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn canonical_like(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn offset_stmt(stmt: &Stmt, offset: usize) -> Stmt {
    let kind = match &stmt.kind {
        StmtKind::Bind {
            name,
            mutable,
            value,
        } => StmtKind::Bind {
            name: name.clone(),
            mutable: *mutable,
            value: offset_expr(value, offset),
        },
        StmtKind::Rebind { name, value } => StmtKind::Rebind {
            name: name.clone(),
            value: offset_expr(value, offset),
        },
        StmtKind::ExprStmt(expr) => StmtKind::ExprStmt(offset_expr(expr, offset)),
        StmtKind::ProcDef {
            name,
            params,
            return_type,
            body,
            is_public,
        } => StmtKind::ProcDef {
            name: name.clone(),
            params: params.iter().map(|p| offset_param(p, offset)).collect(),
            return_type: return_type.clone(),
            body: offset_block(body, offset),
            is_public: *is_public,
        },
        StmtKind::StructDef {
            name,
            fields,
            methods,
            is_public,
        } => StmtKind::StructDef {
            name: name.clone(),
            fields: fields.iter().map(|f| offset_field(f, offset)).collect(),
            methods: methods.iter().map(|m| offset_stmt(m, offset)).collect(),
            is_public: *is_public,
        },
        StmtKind::TraitDef {
            name,
            methods,
            is_public,
        } => StmtKind::TraitDef {
            name: name.clone(),
            methods: methods.iter().map(|m| offset_stmt(m, offset)).collect(),
            is_public: *is_public,
        },
        StmtKind::TraitImpl {
            type_name,
            trait_name,
            methods,
        } => StmtKind::TraitImpl {
            type_name: type_name.clone(),
            trait_name: trait_name.clone(),
            methods: methods.iter().map(|m| offset_stmt(m, offset)).collect(),
        },
        StmtKind::Use { module, items } => StmtKind::Use {
            module: module.clone(),
            items: items.clone(),
        },
        StmtKind::Return(expr) => StmtKind::Return(expr.as_ref().map(|e| offset_expr(e, offset))),
        StmtKind::Continue => StmtKind::Continue,
        StmtKind::Break => StmtKind::Break,
    };

    Stmt {
        kind,
        span: offset_span(stmt.span, offset),
    }
}

fn offset_param(param: &Param, offset: usize) -> Param {
    Param {
        name: param.name.clone(),
        particle: param.particle,
        span: offset_span(param.span, offset),
    }
}

fn offset_field(field: &FieldDef, offset: usize) -> FieldDef {
    FieldDef {
        name: field.name.clone(),
        type_name: field.type_name.clone(),
        span: offset_span(field.span, offset),
    }
}

fn offset_block(block: &Block, offset: usize) -> Block {
    Block {
        statements: block
            .statements
            .iter()
            .map(|s| offset_stmt(s, offset))
            .collect(),
        span: offset_span(block.span, offset),
    }
}

fn offset_expr(expr: &Expr, offset: usize) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Integer(v) => ExprKind::Integer(v.clone()),
        ExprKind::Float(v) => ExprKind::Float(v.clone()),
        ExprKind::StringLiteral(v) => ExprKind::StringLiteral(v.clone()),
        ExprKind::StringInterp(parts) => ExprKind::StringInterp(
            parts
                .iter()
                .map(|p| match p {
                    StringPart::Literal(l) => StringPart::Literal(l.clone()),
                    StringPart::Expr(e) => StringPart::Expr(offset_expr(e, offset)),
                })
                .collect(),
        ),
        ExprKind::Bool(v) => ExprKind::Bool(*v),
        ExprKind::None => ExprKind::None,
        ExprKind::Identifier(name) => ExprKind::Identifier(name.clone()),
        ExprKind::KosoAdo(k) => ExprKind::KosoAdo(*k),
        ExprKind::List(values) => {
            ExprKind::List(values.iter().map(|v| offset_expr(v, offset)).collect())
        }
        ExprKind::Map(entries) => ExprKind::Map(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), offset_expr(v, offset)))
                .collect(),
        ),
        ExprKind::Call { callee, args } => ExprKind::Call {
            callee: callee.clone(),
            args: args
                .iter()
                .map(|a| offset_particle_arg(a, offset))
                .collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(offset_expr(object, offset)),
            property: property.clone(),
        },
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(offset_expr(object, offset)),
            method: method.clone(),
            args: args
                .iter()
                .map(|a| offset_particle_arg(a, offset))
                .collect(),
        },
        ExprKind::BinaryOp { op, left, right } => ExprKind::BinaryOp {
            op: *op,
            left: Box::new(offset_expr(left, offset)),
            right: Box::new(offset_expr(right, offset)),
        },
        ExprKind::UnaryOp { op, operand } => ExprKind::UnaryOp {
            op: *op,
            operand: Box::new(offset_expr(operand, offset)),
        },
        ExprKind::Comparison { op, left, right } => ExprKind::Comparison {
            op: *op,
            left: Box::new(offset_expr(left, offset)),
            right: Box::new(offset_expr(right, offset)),
        },
        ExprKind::Logical { op, left, right } => ExprKind::Logical {
            op: *op,
            left: Box::new(offset_expr(left, offset)),
            right: Box::new(offset_expr(right, offset)),
        },
        ExprKind::If {
            condition,
            then_block,
            elif_clauses,
            else_block,
        } => ExprKind::If {
            condition: Box::new(offset_expr(condition, offset)),
            then_block: offset_block(then_block, offset),
            elif_clauses: elif_clauses
                .iter()
                .map(|(cond, block)| (offset_expr(cond, offset), offset_block(block, offset)))
                .collect(),
            else_block: else_block.as_ref().map(|b| offset_block(b, offset)),
        },
        ExprKind::Match { target, arms } => ExprKind::Match {
            target: Box::new(offset_expr(target, offset)),
            arms: arms
                .iter()
                .map(|arm| offset_match_arm(arm, offset))
                .collect(),
        },
        ExprKind::Loop(kind) => ExprKind::Loop(Box::new(offset_loop_kind(kind, offset))),
        ExprKind::TeChain { steps } => ExprKind::TeChain {
            steps: steps.iter().map(|s| offset_chain_step(s, offset)).collect(),
        },
        ExprKind::BranchChain { if_expr } => ExprKind::BranchChain {
            if_expr: Box::new(offset_expr(if_expr, offset)),
        },
        ExprKind::Lambda { params, body } => ExprKind::Lambda {
            params: params.iter().map(|p| offset_param(p, offset)).collect(),
            body: offset_block(body, offset),
        },
        ExprKind::TryCatch {
            body,
            catch_param,
            catch_body,
            finally_body,
        } => ExprKind::TryCatch {
            body: offset_block(body, offset),
            catch_param: catch_param.clone(),
            catch_body: catch_body.as_ref().map(|b| offset_block(b, offset)),
            finally_body: finally_body.as_ref().map(|b| offset_block(b, offset)),
        },
        ExprKind::Throw(e) => ExprKind::Throw(Box::new(offset_expr(e, offset))),
        ExprKind::WithCounter { value, counter } => ExprKind::WithCounter {
            value: Box::new(offset_expr(value, offset)),
            counter: counter.clone(),
        },
        ExprKind::Construct { type_name, fields } => ExprKind::Construct {
            type_name: type_name.clone(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), offset_expr(v, offset)))
                .collect(),
        },
        ExprKind::Destructure { pattern, value } => ExprKind::Destructure {
            pattern: pattern.clone(),
            value: Box::new(offset_expr(value, offset)),
        },
    };

    Expr {
        kind,
        span: offset_span(expr.span, offset),
    }
}

fn offset_particle_arg(arg: &ParticleArg, offset: usize) -> ParticleArg {
    ParticleArg {
        value: offset_expr(&arg.value, offset),
        particle: arg.particle,
        span: offset_span(arg.span, offset),
    }
}

fn offset_match_arm(arm: &MatchArm, offset: usize) -> MatchArm {
    MatchArm {
        pattern: offset_pattern(&arm.pattern, offset),
        body: offset_block(&arm.body, offset),
        span: offset_span(arm.span, offset),
    }
}

fn offset_pattern(pattern: &Pattern, offset: usize) -> Pattern {
    match pattern {
        Pattern::Literal(expr) => Pattern::Literal(offset_expr(expr, offset)),
        Pattern::Binding(name) => Pattern::Binding(name.clone()),
        Pattern::Wildcard => Pattern::Wildcard,
        Pattern::List(elems) => {
            Pattern::List(elems.iter().map(|p| offset_pattern(p, offset)).collect())
        }
        Pattern::Default => Pattern::Default,
    }
}

fn offset_loop_kind(kind: &LoopKind, offset: usize) -> LoopKind {
    match kind {
        LoopKind::Times { count, var, body } => LoopKind::Times {
            count: offset_expr(count, offset),
            var: var.clone(),
            body: offset_block(body, offset),
        },
        LoopKind::Range {
            from,
            to,
            var,
            body,
        } => LoopKind::Range {
            from: offset_expr(from, offset),
            to: offset_expr(to, offset),
            var: var.clone(),
            body: offset_block(body, offset),
        },
        LoopKind::While { condition, body } => LoopKind::While {
            condition: offset_expr(condition, offset),
            body: offset_block(body, offset),
        },
        LoopKind::ForEach {
            iterable,
            var,
            body,
        } => LoopKind::ForEach {
            iterable: offset_expr(iterable, offset),
            var: var.clone(),
            body: offset_block(body, offset),
        },
    }
}

fn offset_chain_step(step: &ChainStep, offset: usize) -> ChainStep {
    match step {
        ChainStep::Call { callee, args } => ChainStep::Call {
            callee: callee.clone(),
            args: args
                .iter()
                .map(|a| offset_particle_arg(a, offset))
                .collect(),
        },
        ChainStep::Branch { if_expr } => ChainStep::Branch {
            if_expr: offset_expr(if_expr, offset),
        },
    }
}

fn offset_span(span: Span, offset: usize) -> Span {
    Span::new(
        span.start.saturating_add(offset),
        span.end.saturating_add(offset),
    )
}
