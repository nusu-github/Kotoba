use chumsky::error::{Rich, RichPattern, RichReason};
use chumsky::input::Input;
use chumsky::prelude::*;

use crate::common::source::Span;
use crate::frontend::token::{Token, TokenKind};

type CoreSpan = SimpleSpan<usize>;
type CoreError<'a> = Rich<'a, TokenKind, CoreSpan>;

#[derive(Debug, Clone)]
pub struct CoreParseError {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct NormalizedTokenStream {
    pub tokens: Vec<Token>,
    pub errors: Vec<CoreParseError>,
}

/// token+span 入力を chumsky パイプラインで受け直し、
/// 後段パーサが扱うトークン列を正規化して返す。
///
/// 手書きパーサ本体へは既存トークン列を渡しつつ、先行して chumsky で
/// 構造検査（主に区切り記号の整合）を行い、エラーを収集する。
pub fn normalize_token_stream(tokens: Vec<Token>) -> NormalizedTokenStream {
    let tokens = ensure_eof(tokens);
    let (spanned, eoi) = split_token_span(&tokens);

    let core_item = recursive::<_, _, extra::Err<CoreError<'_>>, _, _>(|core_item| {
        let paren_group = core_item
            .clone()
            .repeated()
            .delimited_by(just(TokenKind::LParen), just(TokenKind::RParen))
            .ignored();

        let bracket_group = core_item
            .clone()
            .repeated()
            .delimited_by(just(TokenKind::LBracket), just(TokenKind::RBracket))
            .ignored();

        let brace_group = core_item
            .clone()
            .repeated()
            .delimited_by(just(TokenKind::LBrace), just(TokenKind::RBrace))
            .ignored();

        let interpolation_group = core_item
            .clone()
            .repeated()
            .delimited_by(
                just(TokenKind::StringInterpStart),
                just(TokenKind::StringInterpEnd),
            )
            .ignored();

        let non_structural = select! {
            kind if is_non_structural_token(&kind) => ()
        };

        choice((
            paren_group,
            bracket_group,
            brace_group,
            interpolation_group,
            non_structural,
        ))
    });

    let structural_parser = core_item.repeated().then_ignore(end());
    let (_, mut core_errors): (_, Vec<CoreError<'_>>) = structural_parser
        .parse(spanned.as_slice().split_token_span(eoi))
        .into_output_errors();

    let declaration_slice = {
        let toiu_intro = just::<_, _, extra::Err<CoreError<'_>>>(TokenKind::ToIu)
            .ignore_then(select! { kind => kind })
            .try_map(|kind, span| {
                if is_declaration_target_token(&kind) {
                    Ok(())
                } else {
                    Err(Rich::custom(
                        span,
                        "「という」の後には「手順」「組」「特性」が必要です",
                    ))
                }
            });
        let non_toiu = select! {
            kind if !matches!(kind, TokenKind::ToIu) => ()
        };

        choice((toiu_intro, non_toiu))
    };
    let declaration_parser = declaration_slice.repeated().then_ignore(end());
    let (_, declaration_errors): (_, Vec<CoreError<'_>>) = declaration_parser
        .parse(spanned.as_slice().split_token_span(eoi))
        .into_output_errors();
    core_errors.extend(declaration_errors);

    let reserved_keyword_slice = {
        let reserved = select! {
            kind @ TokenKind::Shinagara => kind,
            kind @ TokenKind::Matsu => kind,
            kind @ TokenKind::Haikeide => kind,
        }
        .try_map(|_, span| {
            Err::<(), _>(Rich::custom(
                span,
                "DGN-006: 未実装機能です（しながら/待つ/背景で）",
            ))
        });
        let non_reserved = select! {
            kind if !matches!(kind, TokenKind::Shinagara | TokenKind::Matsu | TokenKind::Haikeide) => ()
        };
        choice((reserved, non_reserved))
    };
    let reserved_parser = reserved_keyword_slice
        .repeated()
        .then_ignore(end::<_, extra::Err<CoreError<'_>>>());
    let (_, reserved_errors): (_, Vec<CoreError<'_>>) = reserved_parser
        .parse(spanned.as_slice().split_token_span(eoi))
        .into_output_errors();
    core_errors.extend(reserved_errors);

    NormalizedTokenStream {
        tokens,
        errors: dedup_errors(core_errors.into_iter().map(map_core_error).collect()),
    }
}

fn ensure_eof(mut tokens: Vec<Token>) -> Vec<Token> {
    match tokens.last() {
        Some(last) if matches!(last.kind, TokenKind::Eof) => tokens,
        Some(last) => {
            let eof = Span::new(last.span.end, last.span.end);
            tokens.push(Token::new(TokenKind::Eof, eof));
            tokens
        }
        None => vec![Token::new(TokenKind::Eof, Span::new(0, 0))],
    }
}

fn split_token_span(tokens: &[Token]) -> (Vec<(TokenKind, CoreSpan)>, CoreSpan) {
    let spanned = tokens
        .iter()
        .map(|tok| (tok.kind.clone(), to_simple_span(tok.span)))
        .collect::<Vec<_>>();
    let eoi = spanned
        .last()
        .map(|(_, s)| SimpleSpan::new((), s.end..s.end))
        .unwrap_or_else(|| SimpleSpan::new((), 0..0));
    (spanned, eoi)
}

fn is_non_structural_token(kind: &TokenKind) -> bool {
    !matches!(
        kind,
        TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::LBracket
            | TokenKind::RBracket
            | TokenKind::LBrace
            | TokenKind::RBrace
            | TokenKind::StringInterpStart
            | TokenKind::StringInterpEnd
    )
}

fn is_closing_token(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace | TokenKind::StringInterpEnd
    )
}

fn is_declaration_target_token(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Tejun | TokenKind::Kumi | TokenKind::Tokusei
    )
}

fn token_label(kind: &TokenKind) -> String {
    match kind {
        TokenKind::LParen => "（".to_string(),
        TokenKind::RParen => "）".to_string(),
        TokenKind::LBracket => "【".to_string(),
        TokenKind::RBracket => "】".to_string(),
        TokenKind::LBrace => "｛".to_string(),
        TokenKind::RBrace => "｝".to_string(),
        TokenKind::StringInterpStart => "【".to_string(),
        TokenKind::StringInterpEnd => "】".to_string(),
        TokenKind::Eof => "入力終端".to_string(),
        _ => kind.to_string(),
    }
}

fn map_core_error(error: CoreError<'_>) -> CoreParseError {
    let span = Span::new(error.span().start, error.span().end);
    let message = match error.reason() {
        RichReason::Custom(msg) => format!("構文エラー: {msg}"),
        RichReason::ExpectedFound { .. } => map_expected_found_error(&error),
    };
    CoreParseError { message, span }
}

fn map_expected_found_error(error: &CoreError<'_>) -> String {
    if let Some(found) = error.found() {
        if is_closing_token(found) {
            return format!(
                "構文エラー: 対応する開始記号がない閉じ記号「{}」があります",
                token_label(found)
            );
        }
    }

    if error.found().is_none()
        && let Some(expected_close) = error.expected().find_map(expected_closing_token)
    {
        return format!(
            "構文エラー: 閉じ記号「{}」が不足しています",
            token_label(&expected_close)
        );
    }

    let mut expected = error
        .expected()
        .filter_map(expected_pattern_label)
        .collect::<Vec<_>>();
    expected.sort();
    expected.dedup();
    if expected.len() > 3 {
        expected.truncate(3);
    }

    let found = error
        .found()
        .map(|kind| format!("「{}」", token_label(kind)))
        .unwrap_or_else(|| "入力終端".to_string());

    if expected.is_empty() {
        format!("構文エラー: {found} はこの位置では解釈できません")
    } else {
        format!(
            "構文エラー: {} が必要ですが、{found} がありました",
            expected.join(" または ")
        )
    }
}

fn expected_closing_token(pattern: &RichPattern<'_, TokenKind>) -> Option<TokenKind> {
    match pattern {
        RichPattern::Token(token) if is_closing_token(&**token) => Some((**token).clone()),
        _ => None,
    }
}

fn expected_pattern_label(pattern: &RichPattern<'_, TokenKind>) -> Option<String> {
    match pattern {
        RichPattern::Token(token) => Some(format!("「{}」", token_label(&**token))),
        RichPattern::Label(label) => Some(label.to_string()),
        RichPattern::Identifier(ident) => Some(format!("識別子({ident})")),
        RichPattern::Any | RichPattern::SomethingElse => None,
        RichPattern::EndOfInput => Some("入力終端".to_string()),
        _ => None,
    }
}

fn dedup_errors(errors: Vec<CoreParseError>) -> Vec<CoreParseError> {
    let mut deduped = Vec::new();
    for error in errors {
        if deduped
            .iter()
            .any(|seen: &CoreParseError| seen.message == error.message && seen.span == error.span)
        {
            continue;
        }
        deduped.push(error);
    }
    deduped
}

fn to_simple_span(span: Span) -> CoreSpan {
    SimpleSpan::new((), span.start..span.end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;

    #[test]
    fn core_accepts_balanced_delimiters() {
        let (tokens, lex_errs) = Lexer::new("（1）").tokenize();
        assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");

        let normalized = normalize_token_stream(tokens);
        assert!(
            normalized.errors.is_empty(),
            "errors={:?}",
            normalized.errors
        );
    }

    #[test]
    fn core_reports_unmatched_closing_delimiter() {
        let (tokens, lex_errs) = Lexer::new("）").tokenize();
        assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");

        let normalized = normalize_token_stream(tokens);
        let joined = normalized
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("対応する開始記号がない閉じ記号"),
            "errors={joined}"
        );
    }

    #[test]
    fn core_reports_missing_closing_delimiter() {
        let (tokens, lex_errs) = Lexer::new("（1").tokenize();
        assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");

        let normalized = normalize_token_stream(tokens);
        let joined = normalized
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("閉じ記号"), "errors={joined}");
    }

    #[test]
    fn core_reports_invalid_declaration_target_after_toiu() {
        let (tokens, lex_errs) = Lexer::new("足す という 変数").tokenize();
        assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");

        let normalized = normalize_token_stream(tokens);
        let joined = normalized
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("「という」の後には「手順」「組」「特性」が必要です"),
            "errors={joined}"
        );
    }

    #[test]
    fn core_accepts_valid_declaration_target_after_toiu() {
        let (tokens, lex_errs) = Lexer::new("足す という 手順").tokenize();
        assert!(lex_errs.is_empty(), "lex_errs={lex_errs:?}");

        let normalized = normalize_token_stream(tokens);
        let joined = normalized
            .errors
            .iter()
            .map(|e| e.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !joined.contains("「という」の後には「手順」「組」「特性」が必要です"),
            "errors={joined}"
        );
    }
}
