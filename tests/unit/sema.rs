use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use kotoba::sema::infer::analyze;

#[test]
fn sema_rejects_standalone_kosoado() {
    let (tokens, lex_errs) = Lexer::new("これ").tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let diags = analyze(&program);
    assert!(!diags.is_empty());
}
