use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

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
    order: Vec<PathBuf>,
    temp_stack: Vec<PathBuf>,
    temp_set: HashSet<PathBuf>,
    done: HashSet<PathBuf>,
    policies: HashMap<PathBuf, ImportPolicy>,
    diagnostics: Vec<ModuleDiagnostic>,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
            order: Vec::new(),
            temp_stack: Vec::new(),
            temp_set: HashSet::new(),
            done: HashSet::new(),
            policies: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    fn visit(&mut self, path: PathBuf) {
        if self.done.contains(&path) {
            return;
        }

        if self.temp_set.contains(&path) {
            let mut cycle = Vec::new();
            if let Some(idx) = self.temp_stack.iter().position(|p| p == &path) {
                cycle.extend(self.temp_stack[idx..].iter().cloned());
                cycle.push(path.clone());
            }
            self.diagnostics.push(ModuleDiagnostic {
                diagnostic: Diagnostic::new(
                    DiagnosticKind::Compile,
                    format!(
                        "モジュール循環参照を検出しました: {}",
                        cycle
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(" -> ")
                    ),
                )
                .with_hint("`使う` の依存関係を非循環（DAG）にしてください"),
                source: None,
            });
            return;
        }

        self.temp_set.insert(path.clone());
        self.temp_stack.push(path.clone());

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
                self.temp_set.remove(&path);
                self.temp_stack.pop();
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
            self.temp_set.remove(&path);
            self.temp_stack.pop();
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
            self.temp_set.remove(&path);
            self.temp_stack.pop();
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
            self.visit(dep);
        }

        self.temp_set.remove(&path);
        self.temp_stack.pop();
        self.done.insert(path.clone());
        self.order.push(path);
    }
}

#[derive(Debug, Clone)]
struct UseStmt {
    module: String,
    items: Option<Vec<String>>,
}

pub fn resolve_root_program(root: &Path) -> Result<ResolvedProgram, Vec<ModuleDiagnostic>> {
    let root_path = canonical_like(root.to_path_buf());

    let mut builder = GraphBuilder::new();
    builder.visit(root_path.clone());

    if !builder.diagnostics.is_empty() {
        return Err(builder.diagnostics);
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

    let mut merged = Vec::new();

    // 依存モジュールの公開定義を先に導入。
    for module_path in &builder.order {
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
