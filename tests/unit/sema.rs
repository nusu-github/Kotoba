use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use kotoba::sema::infer::{analyze, analyze_typed, analyze_with_limit};
use kotoba::sema::types::Type;

#[test]
fn sema_rejects_standalone_kosoado() {
    let (tokens, lex_errs) = Lexer::new("これ").tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("DGN-002"), "diags={joined}");
    assert!(
        joined.contains("これ は単独では使えません。これの識別子を使用してください"),
        "diags={joined}"
    );
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

#[test]
fn sema_stops_when_step_limit_exceeded() {
    let src = r#"
値 は 1
もし 真 ならば
  値 は 値と1の和
そうでなければ
  値 は 値と2の和
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty());

    let diags = analyze_with_limit(&program, 1);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("解析回数が上限を超えた"), "diags={joined}");
}

#[test]
fn sema_rejects_incomplete_rebind_call() {
    let src = "数を 1 変える";
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
    assert!(joined.contains("変える"), "diags={joined}");
}

#[test]
fn sema_rejects_input_with_arguments() {
    let src = "「x」を 入力する";
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
    assert!(joined.contains("入力する"), "diags={joined}");
}

#[test]
fn sema_rejects_invalid_read_signature() {
    let src = "「a.txt」を 「b.txt」に 読む";
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
    assert!(joined.contains("読む"), "diags={joined}");
}

#[test]
fn sema_rejects_invalid_write_signature() {
    let src = "「x」を 書く";
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
    assert!(joined.contains("書く"), "diags={joined}");
}

#[test]
fn sema_rejects_kou_outside_proc() {
    let (tokens, lex_errs) = Lexer::new("こう").tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    let diags = analyze(&program);
    let joined = diags
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("こう"), "diags={joined}");
}

#[test]
fn sema_allows_kou_inside_proc() {
    let src = "階乗 という 手順\n  こう";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    let diags = analyze(&program);
    assert!(
        diags.is_empty(),
        "unexpected diagnostics: {}",
        diags
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn sema_rejects_kore_inside_te_chain_branch() {
    let src = "「a.csv」を 読んで、分岐して\n  もし 真 ならば\n    これを 返す\n  そうでなければ\n    「ok」を 返す\n表示する";
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
    assert!(joined.contains("DGN-002"), "diags={joined}");
}

#[test]
fn sema_builds_typed_hir_symbol_types() {
    let src = r#"
秒数 は 1秒
合計 は 秒数と2秒の和
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");

    let typed = analyze_typed(&program).expect("typed hir");
    let seconds = typed.symbols.lookup("秒数").expect("秒数 symbol");
    let total = typed.symbols.lookup("合計").expect("合計 symbol");
    assert_eq!(seconds.ty, Type::NumberWithDimension("秒".to_string()));
    assert_eq!(total.ty, Type::NumberWithDimension("秒".to_string()));
}

#[test]
fn sema_typed_infers_io_builtin_types() {
    let src = r#"
名前 は 入力する
内容 は 「in.txt」を 読む
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");

    let typed = analyze_typed(&program).expect("typed hir");
    let name = typed.symbols.lookup("名前").expect("名前 symbol");
    let content = typed.symbols.lookup("内容").expect("内容 symbol");
    assert_eq!(name.ty, Type::String);
    assert_eq!(content.ty, Type::String);
}

#[test]
fn sema_typed_rejects_rebind_type_mismatch() {
    let src = r#"
値 は 真
値を 1に 変える
"#;
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty());
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");

    let err = analyze_typed(&program).expect_err("type mismatch should be rejected");
    let joined = err
        .iter()
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("型制約"), "diags={joined}");
}
