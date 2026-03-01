use kotoba::frontend::ast::StmtKind;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;

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

#[test]
fn parser_accepts_try_as_expression_in_binding() {
    let src = "結果 は 試す\n  1\n失敗した場合\n  2";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_reports_dgn_005_for_invalid_public_target() {
    let src = "公開 名前は「太郎」";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty());
    let joined = parse_errs
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("DGN-005"), "parse_errs={joined}");
}

#[test]
fn parser_reports_dgn_006_for_reserved_future_keyword() {
    let src = "しながら";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty());
    let joined = parse_errs
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("DGN-006"), "parse_errs={joined}");
}

#[test]
fn parser_accepts_proc_with_if_body() {
    let src = r#"
減らす という 手順【n:を】
  もし nが0と等しい ならば
    0
  そうでなければ
    1
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}
