use kotoba::backend::codegen::Compiler;
use kotoba::backend::vm::VM;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;

#[test]
fn vm_executes_addition() {
    let src = "x は 1\ny は 2\nxとyの和と 表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let chunks = Compiler::new().compile(&program).expect("compile failed");
    let mut vm = VM::new(chunks);
    assert!(vm.run().is_ok());
    assert_eq!(vm.output, vec!["3"]);
}
