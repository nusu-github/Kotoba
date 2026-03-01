use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use kotoba::frontend::ast::StmtKind;

#[test]
fn parser_accepts_proc_def() {
    let src = "挨拶する という 手順【名前:を】\n  名前を 表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
}

#[test]
fn parser_converts_use_statement() {
    let src = "「数学」を 使う";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0].kind {
        StmtKind::Use { module, items } => {
            assert_eq!(module, "数学");
            assert!(items.is_none());
        }
        other => panic!("Use文が期待されました: {:?}", other),
    }
}

#[test]
fn parser_converts_from_use_statement() {
    let src = "「数学」から 「平方根」を 使う";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0].kind {
        StmtKind::Use { module, items } => {
            assert_eq!(module, "数学");
            let items = items.as_ref().expect("items");
            assert_eq!(items, &vec!["平方根".to_string()]);
        }
        other => panic!("Use文が期待されました: {:?}", other),
    }
}
