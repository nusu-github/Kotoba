use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser as ClapParser, Subcommand};
use serde::Deserialize;
use tracing::{debug, info, info_span, instrument};
use tracing_subscriber::EnvFilter;

use kotoba::backend::codegen::Compiler;
use kotoba::backend::rir::{RegProgram, RirProgram};
use kotoba::backend::vm::RegVM;
use kotoba::common::source::{SourceFile, Span};
use kotoba::diag::report::{Diagnostic, DiagnosticKind, render};
use kotoba::frontend::ast::Program;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use kotoba::module::resolver::{MappedSource, ModuleDiagnostic, resolve_root_program};
use kotoba::sema::infer::analyze_typed;

#[derive(Debug, ClapParser)]
#[command(name = "kotoba")]
#[command(about = "言（ことば）v1 ツールチェーン", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 実行する
    Run {
        file: PathBuf,
        #[arg(long)]
        debug_ir: bool,
        #[arg(long)]
        debug_vm: bool,
    },
    /// 静的検証のみ行う
    Check { file: PathBuf },
    /// 適合ケースを実行する
    Test {
        #[arg(long)]
        filter: Option<String>,
    },
}

#[derive(Debug)]
struct CompileArtifacts {
    source_file: SourceFile,
    reg_program: RegProgram,
}

#[derive(Debug, Clone)]
struct DiagEntry {
    diagnostic: Diagnostic,
    source: Option<SourceFile>,
}

fn map_diags_with_sources(
    mut diags: Vec<Diagnostic>,
    default_source: &SourceFile,
    source_map: Option<&[MappedSource]>,
) -> Vec<DiagEntry> {
    diags
        .drain(..)
        .map(|mut diagnostic| {
            let mapped = diagnostic
                .span
                .and_then(|span| source_map.and_then(|m| remap_span(span, m)));

            let source = if let Some((local_span, src)) = mapped {
                diagnostic.span = Some(local_span);
                src
            } else {
                default_source.clone()
            };

            DiagEntry {
                diagnostic,
                source: Some(source),
            }
        })
        .collect()
}

fn remap_span(span: Span, source_map: &[MappedSource]) -> Option<(Span, SourceFile)> {
    source_map
        .iter()
        .find(|s| {
            let end = s.base + s.source.content.len();
            span.start >= s.base && span.end <= end
        })
        .map(|s| {
            (
                Span::new(
                    span.start.saturating_sub(s.base),
                    span.end.saturating_sub(s.base),
                ),
                s.source.clone(),
            )
        })
}

#[derive(Debug, Deserialize)]
struct Manifest {
    cases: Vec<Case>,
    #[serde(default)]
    catalog: Vec<CatalogCase>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    mode: String,
    expect: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct CatalogCase {
    id: String,
}

#[derive(Debug)]
enum CaseEval {
    Pass,
    Fail {
        reason: String,
        diags: Vec<DiagEntry>,
    },
}

fn main() {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            file,
            debug_ir: _,
            debug_vm,
        } => run_cmd(file, debug_vm),
        Command::Check { file } => check_cmd(file),
        Command::Test { filter } => test_cmd(filter),
    }
}

fn render_entries(entries: &[DiagEntry]) {
    for e in entries {
        render(&e.diagnostic, e.source.as_ref());
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn compile_program(
    program: &Program,
    source_file: SourceFile,
    source_map: Option<&[MappedSource]>,
) -> Result<CompileArtifacts, Vec<DiagEntry>> {
    let _span = info_span!("compile_program").entered();
    let typed = match analyze_typed(program) {
        Ok(typed) => typed,
        Err(sema_errors) => {
            return Err(map_diags_with_sources(
                sema_errors,
                &source_file,
                source_map,
            ));
        }
    };

    let chunks = match Compiler::new().compile_typed(&typed) {
        Ok(chunks) => chunks,
        Err(errors) => {
            let diags = errors
                .into_iter()
                .map(|e| DiagEntry {
                    diagnostic: Diagnostic::new(DiagnosticKind::Compile, e.message)
                        .with_hint("意味規則または未実装機能を確認してください"),
                    source: None,
                })
                .collect();
            return Err(diags);
        }
    };
    let rir = RirProgram::from_chunks(&chunks);
    let reg_program = rir.into_reg_program();

    Ok(CompileArtifacts {
        source_file,
        reg_program,
    })
}

fn static_check_program(
    program: &Program,
    source_file: SourceFile,
    source_map: Option<&[MappedSource]>,
) -> Result<(), Vec<DiagEntry>> {
    let _span = info_span!("static_check_program").entered();
    if let Err(sema_errors) = analyze_typed(program) {
        return Err(map_diags_with_sources(
            sema_errors,
            &source_file,
            source_map,
        ));
    };
    Ok(())
}

#[instrument(skip(source_code))]
fn compile_text(name: String, source_code: String) -> Result<CompileArtifacts, Vec<DiagEntry>> {
    let source_file = SourceFile::new(name, source_code);
    debug!(source = %source_file.name, bytes = source_file.content.len(), "source loaded");

    let (tokens, lex_errors) = Lexer::new(&source_file.content).tokenize();
    debug!(
        token_count = tokens.len(),
        lex_errors = lex_errors.len(),
        "lex finished"
    );
    if !lex_errors.is_empty() {
        let diags = lex_errors
            .into_iter()
            .map(|e| DiagEntry {
                diagnostic: Diagnostic::new(DiagnosticKind::Lex, e.message)
                    .with_span(e.span)
                    .with_hint("字句規則に沿って入力を修正してください"),
                source: Some(source_file.clone()),
            })
            .collect();
        return Err(diags);
    }

    let (program, parse_errors) = Parser::new(tokens).parse();
    debug!(
        stmt_count = program.statements.len(),
        parse_errors = parse_errors.len(),
        "parse finished"
    );
    if !parse_errors.is_empty() {
        let diags = parse_errors
            .into_iter()
            .map(|e| DiagEntry {
                diagnostic: Diagnostic::new(DiagnosticKind::Parse, e.message)
                    .with_span(e.span)
                    .with_hint("構文を確認してください"),
                source: Some(source_file.clone()),
            })
            .collect();
        return Err(diags);
    }

    compile_program(&program, source_file, None)
}

fn check_text(name: String, source_code: String) -> Result<(), Vec<DiagEntry>> {
    let source_file = SourceFile::new(name, source_code);
    debug!(source = %source_file.name, bytes = source_file.content.len(), "source loaded");

    let (tokens, lex_errors) = Lexer::new(&source_file.content).tokenize();
    debug!(
        token_count = tokens.len(),
        lex_errors = lex_errors.len(),
        "lex finished"
    );
    if !lex_errors.is_empty() {
        let diags = lex_errors
            .into_iter()
            .map(|e| DiagEntry {
                diagnostic: Diagnostic::new(DiagnosticKind::Lex, e.message)
                    .with_span(e.span)
                    .with_hint("字句規則に沿って入力を修正してください"),
                source: Some(source_file.clone()),
            })
            .collect();
        return Err(diags);
    }

    let (program, parse_errors) = Parser::new(tokens).parse();
    debug!(
        stmt_count = program.statements.len(),
        parse_errors = parse_errors.len(),
        "parse finished"
    );
    if !parse_errors.is_empty() {
        let diags = parse_errors
            .into_iter()
            .map(|e| DiagEntry {
                diagnostic: Diagnostic::new(DiagnosticKind::Parse, e.message)
                    .with_span(e.span)
                    .with_hint("構文を確認してください"),
                source: Some(source_file.clone()),
            })
            .collect();
        return Err(diags);
    }

    static_check_program(&program, source_file, None)
}

fn compile_file(path: &PathBuf) -> Result<CompileArtifacts, Vec<DiagEntry>> {
    let _span = info_span!("compile_file", file = %path.display()).entered();
    let resolved = match resolve_root_program(path) {
        Ok(r) => r,
        Err(errs) => {
            return Err(errs
                .into_iter()
                .map(|e: ModuleDiagnostic| DiagEntry {
                    diagnostic: e.diagnostic,
                    source: e.source,
                })
                .collect());
        }
    };

    compile_program(
        &resolved.program,
        resolved.root_source.clone(),
        Some(&resolved.sources),
    )
}

fn check_file(path: &PathBuf) -> Result<(), Vec<DiagEntry>> {
    let _span = info_span!("check_file", file = %path.display()).entered();
    let resolved = match resolve_root_program(path) {
        Ok(r) => r,
        Err(errs) => {
            return Err(errs
                .into_iter()
                .map(|e: ModuleDiagnostic| DiagEntry {
                    diagnostic: e.diagnostic,
                    source: e.source,
                })
                .collect());
        }
    };

    static_check_program(
        &resolved.program,
        resolved.root_source.clone(),
        Some(&resolved.sources),
    )
}

fn run_cmd(file: PathBuf, debug_vm: bool) {
    info!(file = %file.display(), "run command");
    let artifacts = match compile_file(&file) {
        Ok(a) => a,
        Err(diags) => {
            render_entries(&diags);
            process::exit(1);
        }
    };

    if debug_vm {
        for c in artifacts.reg_program.chunks() {
            eprintln!("{}", c.disassemble());
        }
    }

    let mut vm = RegVM::new(artifacts.reg_program);
    if let Err(err) = vm.run() {
        let diag = DiagEntry {
            diagnostic: Diagnostic::new(DiagnosticKind::Runtime, err.to_string())
                .with_hint("実行時の型/値/未実装機能を確認してください"),
            source: Some(artifacts.source_file),
        };
        render_entries(&[diag]);
        process::exit(1);
    }
}

fn check_cmd(file: PathBuf) {
    info!(file = %file.display(), "check command");
    match check_file(&file) {
        Ok(_) => println!("OK"),
        Err(diags) => {
            render_entries(&diags);
            process::exit(1);
        }
    }
}

fn test_cmd(filter: Option<String>) {
    info!(?filter, "test command");
    let manifest_path = std::env::var("KOTOBA_TEST_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("tests/conformance/manifest.yaml"));
    let content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("manifest 読み込み失敗: {}", e);
            process::exit(1);
        }
    };

    let manifest: Manifest = match serde_yaml::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("manifest 解析失敗: {}", e);
            process::exit(1);
        }
    };

    if let Err(msg) = validate_manifest_ids(&manifest) {
        eprintln!("manifest 検証失敗: {msg}");
        process::exit(1);
    }

    let mut total = 0usize;
    let mut passed = 0usize;

    for case in &manifest.cases {
        if let Some(f) = &filter {
            if !case.id.contains(f) {
                continue;
            }
        }

        total += 1;
        let filename = format!("<{}>", case.id);
        let result = match case.mode.as_str() {
            "check" => evaluate_check_case(case, check_text(filename, case.input.clone())),
            "run" => evaluate_run_case(case, compile_text(filename, case.input.clone())),
            _ => CaseEval::Fail {
                reason: format!("manifest の mode が不正です: mode={}", case.mode),
                diags: Vec::new(),
            },
        };

        match result {
            CaseEval::Pass => {
                passed += 1;
                println!("PASS {}", case.id);
            }
            CaseEval::Fail { reason, diags } => {
                println!("FAIL {}", case.id);
                eprintln!("  reason: {}", reason);
                if !diags.is_empty() {
                    render_entries(&diags);
                }
            }
        }
    }

    if manifest.catalog.is_empty() {
        println!("summary: {}/{}", passed, total);
    } else {
        println!(
            "summary: {}/{} (catalog: {})",
            passed,
            total,
            manifest.catalog.len()
        );
    }
    if passed != total {
        process::exit(1);
    }
}

fn evaluate_check_case(case: &Case, result: Result<(), Vec<DiagEntry>>) -> CaseEval {
    match (case.expect.as_str(), result) {
        ("accept", Ok(_)) => CaseEval::Pass,
        ("accept", Err(diags)) => CaseEval::Fail {
            reason: "check で受理期待だったが、静的検証で拒否された".into(),
            diags,
        },
        ("reject", Err(_)) => CaseEval::Pass,
        ("reject", Ok(_)) => CaseEval::Fail {
            reason: "check で拒否期待だったが、静的検証が成功した".into(),
            diags: Vec::new(),
        },
        _ => CaseEval::Fail {
            reason: format!("manifest の expect が不正です: expect={}", case.expect),
            diags: Vec::new(),
        },
    }
}

fn evaluate_run_case(case: &Case, result: Result<CompileArtifacts, Vec<DiagEntry>>) -> CaseEval {
    match (case.expect.as_str(), result) {
        ("accept", Ok(artifacts)) => {
            let mut vm = RegVM::new(artifacts.reg_program);
            match vm.run() {
                Ok(_) => CaseEval::Pass,
                Err(err) => CaseEval::Fail {
                    reason: format!("run で受理期待だったが、実行時エラー: {}", err),
                    diags: Vec::new(),
                },
            }
        }
        ("accept", Err(diags)) => CaseEval::Fail {
            reason: "run で受理期待だったが、コンパイル段階で拒否された".into(),
            diags,
        },
        ("reject", Ok(artifacts)) => {
            let mut vm = RegVM::new(artifacts.reg_program);
            match vm.run() {
                Ok(_) => CaseEval::Fail {
                    reason: "run で拒否期待だったが、実行が成功した".into(),
                    diags: Vec::new(),
                },
                Err(_) => CaseEval::Pass,
            }
        }
        ("reject", Err(_)) => CaseEval::Pass,
        _ => CaseEval::Fail {
            reason: format!("manifest の expect が不正です: expect={}", case.expect),
            diags: Vec::new(),
        },
    }
}

fn validate_manifest_ids(manifest: &Manifest) -> Result<(), String> {
    let mut seen_cases = HashSet::new();
    for c in &manifest.cases {
        if !seen_cases.insert(c.id.clone()) {
            return Err(format!("cases 内で重複ケースID: {}", c.id));
        }
        let trimmed = c.input.trim();
        if trimmed.is_empty() {
            return Err(format!("cases 内で入力が空です: {}", c.id));
        }
        if trimmed == "@" {
            return Err(format!(
                "cases 内にプレースホルダ入力が残っています: {}",
                c.id
            ));
        }
    }
    let mut seen_catalog = HashSet::new();
    for c in &manifest.catalog {
        if !seen_catalog.insert(c.id.clone()) {
            return Err(format!("catalog 内で重複ケースID: {}", c.id));
        }
    }

    // 同一入力が過剰に増えると、規範ごとの差分検証が難しくなるため抑制する。
    const MAX_IDENTICAL_INPUT_CASES: usize = 8;
    let mut by_input: HashMap<&str, Vec<&str>> = HashMap::new();
    for c in &manifest.cases {
        by_input
            .entry(c.input.trim())
            .or_default()
            .push(c.id.as_str());
    }
    for (input, ids) in by_input {
        if ids.len() > MAX_IDENTICAL_INPUT_CASES {
            let preview = input.lines().next().unwrap_or("<empty>");
            return Err(format!(
                "同一入力が過剰です（{}件 > {}件）: [{}] input=`{}`",
                ids.len(),
                MAX_IDENTICAL_INPUT_CASES,
                ids.join(", "),
                preview
            ));
        }
    }
    Ok(())
}
