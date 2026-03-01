use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use petgraph::algo::{kosaraju_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use tracing::{debug, instrument};

use crate::common::source::SourceFile;
use crate::diag::report::{Diagnostic, DiagnosticKind};
use crate::frontend::ast::{Program, Stmt, StmtKind};
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

        merged.extend(exported);
    }

    // ルートの use 文を除いた文を追加。
    for stmt in &root_module.program.statements {
        if !matches!(stmt.kind, StmtKind::Use { .. }) {
            merged.push(stmt.clone());
        }
    }

    let merged_program = Program {
        statements: merged,
        span: root_module.program.span,
    };

    Ok(ResolvedProgram {
        root_source: root_module.source,
        program: merged_program,
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
