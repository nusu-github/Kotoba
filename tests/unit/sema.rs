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

#[test]
fn sema_rejects_particle_set_mismatch() {
    let src = r#"
足す という 手順【左:を、右:に】
  左と右の和を 返す
結果 は 1を 足す
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("DGN-003"), "diags={joined}");
}

#[test]
fn sema_rejects_counter_dimension_mismatch() {
    let src = "値 は 1秒が 2メートル より 大きい";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("DGN-004"), "diags={joined}");
}

#[test]
fn sema_rejects_counter_dimension_mismatch_in_addition() {
    let src = "値 は 1秒と2メートルの和";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("DGN-004"), "diags={joined}");
}

#[test]
fn sema_rejects_continue_outside_loop() {
    let (tokens, lex_errs) = Lexer::new("次へ").tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("次へ"), "diags={joined}");
}

#[test]
fn sema_rejects_break_outside_loop() {
    let (tokens, lex_errs) = Lexer::new("抜ける").tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("抜ける"), "diags={joined}");
}
