use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser as ClapParser, Subcommand};
use serde::Deserialize;

use kotoba::backend::codegen::Compiler;
use kotoba::backend::vm::VM;
use kotoba::common::source::SourceFile;
use kotoba::diag::report::{render, Diagnostic, DiagnosticKind};
use kotoba::frontend::ast::Program;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use kotoba::module::resolver::{resolve_root_program, ModuleDiagnostic};
use kotoba::sema::infer::analyze;

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
    chunks: Vec<kotoba::backend::value::Chunk>,
}

#[derive(Debug, Clone)]
struct DiagEntry {
    diagnostic: Diagnostic,
    source: Option<SourceFile>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    mode: String,
    expect: String,
    input: String,
}

fn main() {
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

fn compile_program(
    program: &Program,
    source_file: SourceFile,
    run_sema: bool,
) -> Result<CompileArtifacts, Vec<DiagEntry>> {
    if run_sema {
        let sema_errors = analyze(program);
        if !sema_errors.is_empty() {
            return Err(sema_errors
                .into_iter()
                .map(|d| DiagEntry {
                    diagnostic: d,
                    source: Some(source_file.clone()),
                })
                .collect());
        }
    }

    let chunks = match Compiler::new().compile(program) {
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

    Ok(CompileArtifacts {
        source_file,
        chunks,
    })
}

fn compile_text(name: String, source_code: String) -> Result<CompileArtifacts, Vec<DiagEntry>> {
    let source_file = SourceFile::new(name, source_code);

    let (tokens, lex_errors) = Lexer::new(&source_file.content).tokenize();
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

    compile_program(&program, source_file, true)
}

fn compile_file(path: &PathBuf) -> Result<CompileArtifacts, Vec<DiagEntry>> {
    let resolved = match resolve_root_program(path) {
        Ok(r) => r,
        Err(errs) => {
            return Err(errs
                .into_iter()
                .map(|e: ModuleDiagnostic| DiagEntry {
                    diagnostic: e.diagnostic,
                    source: e.source,
                })
                .collect())
        }
    };

    compile_program(&resolved.program, resolved.root_source, false)
}

fn run_cmd(file: PathBuf, debug_vm: bool) {
    let artifacts = match compile_file(&file) {
        Ok(a) => a,
        Err(diags) => {
            render_entries(&diags);
            process::exit(1);
        }
    };

    if debug_vm {
        for c in &artifacts.chunks {
            eprintln!("{}", c.disassemble());
        }
    }

    let mut vm = VM::new(artifacts.chunks);
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
    match compile_file(&file) {
        Ok(_) => println!("OK"),
        Err(diags) => {
            render_entries(&diags);
            process::exit(1);
        }
    }
}

fn test_cmd(filter: Option<String>) {
    let manifest_path = PathBuf::from("tests/conformance/manifest.yaml");
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
        let result = compile_text(filename, case.input.clone());

        let ok = match (case.mode.as_str(), case.expect.as_str(), result) {
            ("check", "accept", Ok(_)) => true,
            ("check", "reject", Err(_)) => true,
            ("run", "accept", Ok(artifacts)) => {
                let mut vm = VM::new(artifacts.chunks);
                vm.run().is_ok()
            }
            ("run", "reject", Ok(artifacts)) => {
                let mut vm = VM::new(artifacts.chunks);
                vm.run().is_err()
            }
            ("run", "reject", Err(_)) => true,
            _ => false,
        };

        if ok {
            passed += 1;
            println!("PASS {}", case.id);
        } else {
            println!("FAIL {}", case.id);
        }
    }

    println!("summary: {}/{}", passed, total);
    if passed != total {
        process::exit(1);
    }
}
