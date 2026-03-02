use std::fs;

use kotoba::backend::codegen::Compiler;
use kotoba::backend::rir::RirProgram;
use kotoba::backend::vm::RegVM;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;

fn run_src(src: &str) -> Result<RegVM, String> {
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    if !lex_errs.is_empty() {
        return Err(format!("lex_errs={lex_errs:?}"));
    }
    let (program, parse_errs) = Parser::new(tokens).parse();
    if !parse_errs.is_empty() {
        return Err(format!("parse_errs={parse_errs:?}"));
    }
    let chunks = Compiler::new()
        .compile(&program)
        .map_err(|e| format!("compile_errs={e:?}"))?;
    let rir = RirProgram::from_chunks(&chunks);
    let mut vm = RegVM::new(rir.into_reg_program());
    vm.run().map_err(|e| e.to_string())?;
    Ok(vm)
}

#[test]
fn vm_executes_addition() {
    let src = "x は 1\ny は 2\nxとyの和と 表示する";
    let vm = run_src(src).expect("run");
    assert_eq!(vm.output(), &["3".to_string()]);
}

#[test]
fn vm_reads_and_writes_file_with_builtins() {
    let path = format!("target/tmp_vm_io_{}.txt", std::process::id());
    let _ = fs::remove_file(&path);
    let src =
        format!("「こんにちは」を 「{path}」に 書く\n内容 は 「{path}」を 読む\n内容と 表示する");
    let vm = run_src(&src).expect("run");
    assert_eq!(vm.output(), &["こんにちは".to_string()]);
    let file = fs::read_to_string(&path).expect("read written file");
    assert_eq!(file, "こんにちは");
    let _ = fs::remove_file(&path);
}

#[test]
fn vm_try_finally_discards_finally_value() {
    let src = r#"
結果 は 試す
  1
必ず行う
  999
結果と 表示する
"#;
    let vm = run_src(src).expect("run");
    assert_eq!(vm.output(), &["1".to_string()]);
}

#[test]
fn vm_try_finally_rethrow_overrides_original() {
    let src = r#"
結果 は 試す
  「元の例外」と 訴える
失敗した場合【e:で】
  e
必ず行う
  「後の例外」と 訴える
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    let chunks = Compiler::new().compile(&program).expect("compile");
    let rir = RirProgram::from_chunks(&chunks);
    let mut vm = RegVM::new(rir.into_reg_program());
    let err = vm.run().expect_err("must throw");
    let msg = err.to_string();
    assert!(msg.contains("後の例外"), "msg={msg}");
}

#[test]
fn vm_supports_kou_recursive_call() {
    let src = r#"
減らす という 手順【n:を】
  もし nが0と等しい ならば
    0を 返す
  そうでなければ
    次 は nと1の差
    次を こう 返す
結果 は 3を 減らす
結果と 表示する
"#;
    let vm = run_src(src).expect("run");
    assert_eq!(vm.output(), &["0".to_string()]);
}

#[test]
fn regvm_runs_program() {
    let src = "x は 1\ny は 2\nxとyの和と 表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    let chunks = Compiler::new().compile(&program).expect("compile");
    let rir = RirProgram::from_chunks(&chunks);
    let mut vm = RegVM::new(rir.into_reg_program());
    vm.run().expect("run regvm");
    assert_eq!(vm.output(), &["3".to_string()]);
}
