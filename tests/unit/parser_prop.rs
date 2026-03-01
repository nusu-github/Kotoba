use kotoba::frontend::lexer::Lexer;
use kotoba::frontend::parser::Parser;
use proptest::prelude::*;

proptest! {
    #[test]
    fn parser_never_panics_on_any_unicode_input(input in ".*") {
        let (tokens, _lex_errs) = Lexer::new(&input).tokenize();
        let token_count = tokens.len();
        let (program, parse_errs) = Parser::new(tokens).parse();
        prop_assert!(program.span.start <= program.span.end);
        prop_assert!(program.statements.len() <= token_count);
        for stmt in &program.statements {
            prop_assert!(stmt.span.start <= stmt.span.end);
        }
        for err in &parse_errs {
            prop_assert!(err.span.start <= err.span.end);
        }
    }
}
