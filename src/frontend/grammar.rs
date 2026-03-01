use chumsky::prelude::*;

// v1 で chumsky を正式採用するための最小エントリポイント。
// 詳細文法は frontend/parser.rs へ段階移行する。
pub fn sanity_parser<'src>() -> impl Parser<'src, &'src str, (), extra::Err<Simple<'src, char>>> {
    just::<_, _, extra::Err<Simple<char>>>("言")
        .ignored()
        .or_not()
        .ignored()
}
