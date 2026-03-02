use kotoba::common::source::Span;
use kotoba::frontend::ast::{BinOp, ExprKind, {ExprKind, StmtKind}};
use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use kotoba::frontend::token::{Particle, Token, TokenKind};

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
fn parser_reports_invalid_declaration_target_after_toiu() {
    let src = "足す という 変数";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
    let joined = parse_errs
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("「という」の後には「手順」「組」「特性」が必要です"),
        "parse_errs={joined}"
    );
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

#[test]
fn parser_does_not_hang_on_invalid_proc_param_syntax() {
    let src = "表示する という 手順【:を】\n  返す";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
}

#[test]
fn parser_rejects_try_catch_param_particle_not_de() {
    let src = "試す\n  1\n失敗した場合【問題:を】\n  0";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
}

#[test]
fn parser_rejects_duplicate_public_modifier() {
    let src = "公開 公開 足す という 手順【a:を】\n  aを返す";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
}

#[test]
fn parser_rejects_missing_statement_separator() {
    let src = "名前 「太郎」";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
}

#[test]
fn parser_reports_unmatched_closing_delimiter_and_recovers() {
    let src = "）\n表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
    let joined = parse_errs
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("対応する開始記号がない閉じ記号「）」"),
        "parse_errs={joined}"
    );
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn parser_reports_missing_closing_delimiter_and_recovers() {
    let src = "値は（1\n表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
    let joined = parse_errs
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("閉じ記号「）」が不足しています"),
        "parse_errs={joined}"
    );
    assert!(
        !program.statements.is_empty(),
        "program.statements={:?}",
        program.statements
    );
}

#[test]
fn parser_lowers_mod_expression_in_comparison() {
    let src = "もし iを 2で 割った余りが0と等しい ならば\n  1\n";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");

    let stmt = program
        .statements
        .first()
        .expect("expected one statement in program");
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        panic!("if statement expected, got {:?}", stmt.kind);
    };
    let ExprKind::If { condition, .. } = &expr.kind else {
        panic!("if expression expected, got {:?}", expr.kind);
    };
    let ExprKind::Comparison { left, .. } = &condition.kind else {
        panic!("comparison expected, got {:?}", condition.kind);
    };
    match &left.kind {
        ExprKind::BinaryOp { op, .. } => assert_eq!(*op, BinOp::Mod),
        other => panic!("mod binary op expected, got {:?}", other),
    }
}

#[test]
fn parser_recovers_and_keeps_following_valid_statement() {
    let src = "名前 「太郎」\n表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn parser_accepts_tokens_without_explicit_eof() {
    let tokens = vec![Token::new(TokenKind::HyoujiSuru, Span::new(0, 0))];
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn parser_rejects_access_particle_as_role_on_string() {
    let src = "「x」の 複製する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
}

#[test]
fn parser_rejects_trait_impl_without_body() {
    let src = "人 は 表示できる を持つ";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(!parse_errs.is_empty(), "expected parse errors");
}

#[test]
fn parser_accepts_mutable_bind_without_space_before_value() {
    let src = "変わる 数は0";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0].kind {
        StmtKind::Bind { name, mutable, .. } => {
            assert_eq!(name, "数");
            assert!(*mutable);
        }
        other => panic!("可変束縛が期待されました: {:?}", other),
    }
}

#[test]
fn parser_accepts_trait_method_signature_without_body() {
    let src = "表示できる という 特性\n  文字列にする という 手順";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_trait_impl_with_body() {
    let src = "人は 表示できる を持つ\n  文字列にする という 手順\n    返す";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_foreach_loop() {
    let src = "一覧の それぞれについて【e】\n  eを 表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_bare_display_call_statement() {
    let src = "表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_bare_input_call_statement() {
    let src = "入力する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_match_expression_with_literal_and_default_arm() {
    let src = "値は どれかを調べる\n  1の場合\n    10\n  どれでもない場合\n    0";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_match_expression_with_list_pattern_arm() {
    let src = "値は どれかを調べる\n  【どれか、x】の場合\n    x\n  どれでもない場合\n    0";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_te_chain() {
    let src = "「a.csv」を 読んで、行に分けて、表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_accepts_te_chain_with_branch() {
    let src = "「a.csv」を 読んで、分岐して\n  もし 真 ならば\n    「成功」を 返す\n  そうでなければ\n    「失敗」を 返す\n表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (_program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
}

#[test]
fn parser_chains_direct_call_result_into_display() {
    let src = "見出しを 飾ると 表示する";
    let (tokens, lex_errs) = Lexer::new(src).tokenize();
    assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");
    let (program, parse_errs) = Parser::new(tokens).parse();
    assert!(parse_errs.is_empty(), "parse_errs={parse_errs:?}");
    assert_eq!(program.statements.len(), 1);

    let StmtKind::ExprStmt(expr) = &program.statements[0].kind else {
        panic!("ExprStmt expected: {:?}", program.statements[0].kind);
    };

    let ExprKind::Call { callee, args } = &expr.kind else {
        panic!("Call expected: {:?}", expr.kind);
    };
    assert_eq!(callee, "表示する");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].particle, Particle::To);

    let ExprKind::Call {
        callee: inner_callee,
        args: inner_args,
    } = &args[0].value.kind
    else {
        panic!("nested Call expected: {:?}", args[0].value.kind);
    };
    assert_eq!(inner_callee, "飾る");
    assert_eq!(inner_args.len(), 1);
    assert_eq!(inner_args[0].particle, Particle::Wo);
}
