use insta::assert_snapshot;
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;

fn normalize_newline(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn frontend_lexer_tokens_core_snapshot() {
    let src = "\
公開 足す という 手順【a:を、b:を】
  結果 は aとbの和
  結果を 返す
「数学」から 「平方根」を 使う
";
    let (tokens, lex_errors) = Lexer::new(src).tokenize();
    assert!(lex_errors.is_empty(), "lex_errors={lex_errors:?}");

    let rendered = tokens
        .iter()
        .map(|tok| format!("{:?} @{}..{}", tok.kind, tok.span.start, tok.span.end))
        .collect::<Vec<_>>()
        .join("\n");

    insta::with_settings!({prepend_module_to_snapshot => false}, {
        assert_snapshot!("frontend_lexer_tokens_core", normalize_newline(&rendered));
    });
}

#[test]
fn frontend_parser_ast_core_snapshot() {
    let src = "\
足す という 手順【a:を、b:を】
  合計 は aとbの和
  合計を 返す
";
    let (tokens, lex_errors) = Lexer::new(src).tokenize();
    assert!(lex_errors.is_empty(), "lex_errors={lex_errors:?}");

    let (program, parse_errors) = Parser::new(tokens).parse();
    assert!(parse_errors.is_empty(), "parse_errors={parse_errors:?}");

    insta::with_settings!({prepend_module_to_snapshot => false}, {
        assert_snapshot!(
            "frontend_parser_ast_core",
            normalize_newline(&format!("{program:#?}"))
        );
    });
}

#[test]
fn frontend_parser_errors_core_snapshot() {
    let src = "名前 「太郎」";
    let (tokens, lex_errors) = Lexer::new(src).tokenize();
    assert!(lex_errors.is_empty(), "lex_errors={lex_errors:?}");

    let (_program, parse_errors) = Parser::new(tokens).parse();
    assert!(!parse_errors.is_empty(), "expected parse errors");

    let rendered = parse_errors
        .iter()
        .map(|err| format!("{} @{}..{}", err.message, err.span.start, err.span.end))
        .collect::<Vec<_>>()
        .join("\n");

    insta::with_settings!({prepend_module_to_snapshot => false}, {
        assert_snapshot!("frontend_parser_errors_core", normalize_newline(&rendered));
    });
}
