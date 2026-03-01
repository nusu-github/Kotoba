use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::token::TokenKind;

#[test]
fn lexer_splits_attached_particle() {
    let (tokens, errs) = Lexer::new("名前を 表示する").tokenize();
    assert!(errs.is_empty());
    assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Identifier(ref s) if s == "名前")));
    assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Particle(_))));
}
