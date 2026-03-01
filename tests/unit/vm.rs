use kotoba::backend::codegen::Compiler;
use kotoba::backend::vm::VM;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;

fn run_src(src: &str) -> Result<VM, String> {
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
    let mut vm = VM::new(chunks);
    vm.run().map_err(|e| e.to_string())?;
    Ok(vm)
}

#[test]
fn vm_executes_addition() {
    let src = "x は 1\ny は 2\nxとyの和と 表示する";
    let vm = run_src(src).expect("run");
    assert_eq!(vm.output, vec!["3"]);
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
    assert_eq!(vm.output, vec!["1"]);
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
    let mut vm = VM::new(chunks);
    let err = vm.run().expect_err("must throw");
    let msg = err.to_string();
    assert!(msg.contains("後の例外"), "msg={msg}");
}
