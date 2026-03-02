use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::token::{Particle, TokenKind};

#[test]
fn lexer_splits_attached_particle() {
    let (tokens, errs) = Lexer::new("名前を 表示する").tokenize();
    assert!(errs.is_empty());
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Identifier(ref s) if s == "名前"))
    );
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Particle(_)))
    );
}

#[test]
fn lexer_normalizes_identifier_to_nfc() {
    let src = "か\u{3099}くせい は 真";
    let (tokens, errs) = Lexer::new(src).tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Identifier(ref s) if s == "がくせい"))
    );
}

#[test]
fn lexer_normalizes_keyword_to_nfc() {
    let src = "そうて\u{3099}なければ";
    let (tokens, errs) = Lexer::new(src).tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::SouDenakereba))
    );
}

#[test]
fn lexer_normalizes_identifier_before_access_particle_split() {
    let src = "か\u{3099}くせいの名前";
    let (tokens, errs) = Lexer::new(src).tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    assert!(
        matches!(tokens.first().map(|t| &t.kind), Some(TokenKind::Identifier(s)) if s == "がくせい")
    );
    assert!(matches!(
        tokens.get(1).map(|t| &t.kind),
        Some(TokenKind::AccessParticle)
    ));
    assert!(matches!(
        tokens.get(2).map(|t| &t.kind),
        Some(TokenKind::Identifier(s)) if s == "名前"
    ));
}

#[test]
fn lexer_normalizes_identifier_before_particle_split() {
    let src = "か\u{3099}くせいを 表示する";
    let (tokens, errs) = Lexer::new(src).tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    assert!(
        matches!(tokens.first().map(|t| &t.kind), Some(TokenKind::Identifier(s)) if s == "がくせい")
    );
    assert!(matches!(
        tokens.get(1).map(|t| &t.kind),
        Some(TokenKind::Particle(Particle::Wo))
    ));
}

#[test]
fn lexer_accepts_iteration_mark_in_identifier() {
    let src = "前々 は 1";
    let (tokens, errs) = Lexer::new(src).tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Identifier(ref s) if s == "前々"))
    );
}

#[test]
fn lexer_accepts_ascii_delimiters() {
    let (tokens, errs) = Lexer::new("([{}])").tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    let kinds = tokens
        .into_iter()
        .map(|token| token.kind)
        .collect::<Vec<TokenKind>>();
    assert_eq!(
        kinds,
        vec![
            TokenKind::LParen,
            TokenKind::LBracket,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::RBracket,
            TokenKind::RParen,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn lexer_recognizes_input_keyword() {
    let (tokens, errs) = Lexer::new("入力する").tokenize();
    assert!(errs.is_empty(), "errs={errs:?}");
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::NyuuryokuSuru))
    );
}
