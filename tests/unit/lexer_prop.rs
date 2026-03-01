use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::token::TokenKind;
use proptest::prelude::*;

proptest! {
    #[test]
    fn lexer_always_emits_eof(input in ".*") {
        let (tokens, _errs) = Lexer::new(&input).tokenize();
        prop_assert!(!tokens.is_empty());
        prop_assert!(matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Eof)));
    }

    #[test]
    fn lexer_indent_dedent_stack_never_underflows(input in ".*") {
        let (tokens, _errs) = Lexer::new(&input).tokenize();
        let mut depth: i32 = 0;
        for tok in tokens {
            match tok.kind {
                TokenKind::Indent => depth += 1,
                TokenKind::Dedent => {
                    depth -= 1;
                    prop_assert!(depth >= 0);
                }
                _ => {}
            }
        }
        prop_assert_eq!(depth, 0);
    }
}
