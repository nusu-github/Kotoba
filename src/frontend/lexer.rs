use crate::source::Span;
use crate::token::{Particle, Token, TokenKind};

/// 字句解析エラー
#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

/// 字句解析器
pub struct Lexer<'src> {
    source: &'src str,
    /// 現在のバイトオフセット
    pos: usize,
    /// 生成されたトークン列
    tokens: Vec<Token>,
    /// インデントスタック（各レベルのスペース数）
    indent_stack: Vec<usize>,
    /// 行頭かどうか
    at_line_start: bool,
    /// エラー一覧
    errors: Vec<LexError>,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            pos: 0,
            tokens: Vec::new(),
            indent_stack: vec![0],
            at_line_start: true,
            errors: Vec::new(),
        }
    }

    /// ソースコード全体をトークン化する
    pub fn tokenize(mut self) -> (Vec<Token>, Vec<LexError>) {
        while !self.is_eof() {
            if self.at_line_start {
                self.handle_indent();
                self.at_line_start = false;
            }

            if self.is_eof() {
                break;
            }

            let ch = self.peek_char().unwrap();

            match ch {
                '\n' => {
                    let start = self.pos;
                    self.advance_char();
                    // 連続する空行はスキップ
                    if !self.tokens.is_empty() {
                        let last = self.tokens.last().map(|t| &t.kind);
                        if last != Some(&TokenKind::Newline) {
                            self.tokens
                                .push(Token::new(TokenKind::Newline, Span::new(start, self.pos)));
                        }
                    }
                    self.at_line_start = true;
                }
                ' ' | '\t' | '\r' | '\u{3000}' => {
                    // 空白スキップ（全角スペースも対応）
                    self.advance_char();
                }
                '※' => self.lex_comment(),
                '（' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::LParen, Span::new(start, self.pos)));
                }
                '「' => self.lex_string(),
                '【' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::LBracket, Span::new(start, self.pos)));
                }
                '】' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::RBracket, Span::new(start, self.pos)));
                }
                '｛' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::LBrace, Span::new(start, self.pos)));
                }
                '｝' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::RBrace, Span::new(start, self.pos)));
                }
                '）' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::RParen, Span::new(start, self.pos)));
                }
                '、' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::Comma, Span::new(start, self.pos)));
                }
                '。' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::Period, Span::new(start, self.pos)));
                }
                ':' | '：' => {
                    let start = self.pos;
                    self.advance_char();
                    self.tokens
                        .push(Token::new(TokenKind::Colon, Span::new(start, self.pos)));
                }
                '…' => {
                    let start = self.pos;
                    self.advance_char();
                    // 継続行: 次の改行まで無視して続行
                    self.tokens
                        .push(Token::new(TokenKind::Ellipsis, Span::new(start, self.pos)));
                }
                _ if is_digit_start(ch) => self.lex_number(),
                _ if is_ident_start(ch) => self.lex_identifier_or_keyword(),
                _ => {
                    let start = self.pos;
                    self.advance_char();
                    let msg = format!("予期しない文字: '{ch}'");
                    self.errors.push(LexError {
                        message: msg.clone(),
                        span: Span::new(start, self.pos),
                    });
                    self.tokens.push(Token::new(
                        TokenKind::Error(msg),
                        Span::new(start, self.pos),
                    ));
                }
            }
        }

        // ファイル末尾で未処理のインデントをDedentで閉じる
        let eof_pos = self.pos;
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.tokens
                .push(Token::new(TokenKind::Dedent, Span::new(eof_pos, eof_pos)));
        }

        self.tokens
            .push(Token::new(TokenKind::Eof, Span::new(eof_pos, eof_pos)));

        (self.tokens, self.errors)
    }

    // === ヘルパーメソッド ===

    fn is_eof(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.source[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    #[allow(dead_code)]
    fn remaining(&self) -> &'src str {
        &self.source[self.pos..]
    }

    // === インデント処理 ===

    fn handle_indent(&mut self) {
        let start = self.pos;
        let mut spaces = 0usize;

        while let Some(ch) = self.peek_char() {
            match ch {
                ' ' | '\u{3000}' => {
                    // 全角スペースは2スペースとして扱う
                    spaces += if ch == '\u{3000}' { 2 } else { 1 };
                    self.advance_char();
                }
                '\t' => {
                    spaces += 4; // タブは4スペース相当
                    self.advance_char();
                }
                '\n' => {
                    // 空行はインデント変更を引き起こさない
                    return;
                }
                '※' => {
                    // コメント行もインデント変更を引き起こさない
                    return;
                }
                _ => break,
            }
        }

        let current_indent = *self.indent_stack.last().unwrap();

        if spaces > current_indent {
            self.indent_stack.push(spaces);
            self.tokens
                .push(Token::new(TokenKind::Indent, Span::new(start, self.pos)));
        } else if spaces < current_indent {
            while self.indent_stack.len() > 1 && *self.indent_stack.last().unwrap() > spaces {
                self.indent_stack.pop();
                self.tokens
                    .push(Token::new(TokenKind::Dedent, Span::new(start, self.pos)));
            }
            if *self.indent_stack.last().unwrap() != spaces {
                self.errors.push(LexError {
                    message: "インデントが合いません".to_string(),
                    span: Span::new(start, self.pos),
                });
            }
        }
    }

    // === 数値 ===

    fn lex_number(&mut self) {
        let start = self.pos;
        let mut s = String::new();
        let mut is_float = false;

        // 全角数字を半角に変換しながら読み取る
        while let Some(ch) = self.peek_char() {
            if let Some(d) = to_halfwidth_digit(ch) {
                s.push(d);
                self.advance_char();
            } else if ch == '.' || ch == '．' {
                if is_float {
                    break;
                }
                is_float = true;
                s.push('.');
                self.advance_char();
            } else {
                break;
            }
        }

        let num_end = self.pos;

        // 数値直後に助数詞があるか確認
        let counter_start = self.pos;
        let mut counter = String::new();
        while let Some(ch) = self.peek_char() {
            if is_counter_char(ch) {
                counter.push(ch);
                self.advance_char();
            } else {
                break;
            }
        }

        if is_float {
            self.tokens
                .push(Token::new(TokenKind::Float(s), Span::new(start, num_end)));
        } else {
            self.tokens
                .push(Token::new(TokenKind::Integer(s), Span::new(start, num_end)));
        }

        if !counter.is_empty() {
            self.tokens.push(Token::new(
                TokenKind::Counter(counter),
                Span::new(counter_start, self.pos),
            ));
        }
    }

    // === 文字列 ===

    fn lex_string(&mut self) {
        let start = self.pos;
        self.advance_char(); // 「 を消費

        let mut content = String::new();
        let mut has_interp = false;

        while let Some(ch) = self.peek_char() {
            match ch {
                '」' => {
                    self.advance_char();
                    if !has_interp {
                        self.tokens.push(Token::new(
                            TokenKind::String(content),
                            Span::new(start, self.pos),
                        ));
                    } else {
                        // 式展開を含む場合、残りのテキスト部分を出力
                        if !content.is_empty() {
                            self.tokens.push(Token::new(
                                TokenKind::String(content),
                                Span::new(start, self.pos),
                            ));
                        }
                    }
                    return;
                }
                '【' => {
                    // 式展開の開始
                    has_interp = true;
                    if !content.is_empty() {
                        self.tokens.push(Token::new(
                            TokenKind::String(content.clone()),
                            Span::new(start, self.pos),
                        ));
                        content.clear();
                    }
                    let interp_start = self.pos;
                    self.advance_char();
                    self.tokens.push(Token::new(
                        TokenKind::StringInterpStart,
                        Span::new(interp_start, self.pos),
                    ));
                    // 式展開の中身は通常のトークンとして処理される
                    // 閉じ 】 が見つかるまでトークン化する
                    self.lex_interpolation_body();
                }
                _ => {
                    content.push(ch);
                    self.advance_char();
                }
            }
        }

        // 閉じ括弧なしで終了
        self.errors.push(LexError {
            message: "閉じられていない文字列リテラル".to_string(),
            span: Span::new(start, self.pos),
        });
        self.tokens.push(Token::new(
            TokenKind::String(content),
            Span::new(start, self.pos),
        ));
    }

    fn lex_interpolation_body(&mut self) {
        // 【 の後、】が見つかるまでトークン化
        // 簡易実装: 識別子のみ対応（将来的には完全な式を対応する）
        while !self.is_eof() {
            if let Some(ch) = self.peek_char() {
                match ch {
                    '】' => {
                        let end_start = self.pos;
                        self.advance_char();
                        self.tokens.push(Token::new(
                            TokenKind::StringInterpEnd,
                            Span::new(end_start, self.pos),
                        ));
                        return;
                    }
                    ' ' | '\t' | '\r' | '\u{3000}' => {
                        self.advance_char();
                    }
                    _ if is_ident_start(ch) => {
                        self.lex_identifier_or_keyword();
                    }
                    _ if is_digit_start(ch) => {
                        self.lex_number();
                    }
                    _ => {
                        self.advance_char();
                    }
                }
            }
        }
    }

    // === コメント ===

    fn lex_comment(&mut self) {
        let start = self.pos;
        self.advance_char(); // ※ を消費

        // ブロックコメント: ※（ ... ）※
        if matches!(self.peek_char(), Some('（')) {
            self.advance_char(); // （ を消費
            self.lex_block_comment(start);
            return;
        }

        // 空白をスキップ
        while let Some(ch) = self.peek_char() {
            if ch == ' ' || ch == '\t' {
                self.advance_char();
            } else {
                break;
            }
        }

        let mut content = String::new();
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            content.push(ch);
            self.advance_char();
        }

        // コメントはトークンとして保持しない（将来のドキュメント生成用に保持する場合はここを変更）
        // self.tokens.push(Token::new(TokenKind::LineComment(content), Span::new(start, self.pos)));
        let _ = (start, content);
    }

    fn lex_block_comment(&mut self, start: usize) {
        let mut depth = 1;

        while !self.is_eof() {
            if self.starts_with("※（") {
                depth += 1;
                self.advance_char(); // ※
                self.advance_char(); // （
            } else if self.starts_with("）※") {
                depth -= 1;
                self.advance_char(); // ）
                self.advance_char(); // ※
                if depth == 0 {
                    return;
                }
            } else {
                self.advance_char();
            }
        }

        if depth > 0 {
            self.errors.push(LexError {
                message: "閉じられていないブロックコメント（※（...）※）".to_string(),
                span: Span::new(start, self.pos),
            });
        }
    }

    // === 識別子・キーワード ===

    fn lex_identifier_or_keyword(&mut self) {
        let start = self.pos;
        let mut word = String::new();

        // 最初の文字でASCIモード/日本語モードを判定
        let first_ch = self.peek_char().unwrap();
        let ascii_mode = first_ch.is_ascii_alphabetic() || first_ch == '_';

        while let Some(ch) = self.peek_char() {
            if ascii_mode {
                // ASCIIモード: ASCII英数字とアンダースコアのみ収集
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    word.push(ch);
                    self.advance_char();
                } else {
                    break;
                }
            } else {
                // 日本語モード: 日本語文字のみ収集（ASCII文字で停止）
                if is_japanese_char(ch) {
                    word.push(ch);
                    self.advance_char();
                } else {
                    break;
                }
            }
        }

        let span = Span::new(start, self.pos);

        // キーワード判定（完全一致が最優先）
        if let Some(kind) = keyword_from_str(&word) {
            self.tokens.push(Token::new(kind, span));
            return;
        }

        // アクセス助詞「の」の単体判定
        if word == "の" {
            self.tokens
                .push(Token::new(TokenKind::AccessParticle, span));
            return;
        }

        // 助詞単体の判定（助詞だけを書いた場合）
        if let Some(p) = standalone_particle(&word) {
            self.tokens.push(Token::new(TokenKind::Particle(p), span));
            return;
        }

        // 「の」境界での分割を試行
        // 「の」はアクセス助詞として常に単語を分割する
        // ただしキーワード内の「の」は除外（このチェックは上記のキーワード判定で処理済み）
        if let Some(no_byte_pos) = word.find("の") {
            // 「の」の前の部分
            if no_byte_pos > 0 {
                let before = &word[..no_byte_pos];
                let before_span = Span::new(start, start + no_byte_pos);
                if let Some(kind) = keyword_from_str(before) {
                    self.tokens.push(Token::new(kind, before_span));
                } else {
                    self.tokens.push(Token::new(
                        TokenKind::Identifier(before.to_string()),
                        before_span,
                    ));
                }
            }

            // 「の」→ AccessParticle
            let no_start = start + no_byte_pos;
            let no_end = no_start + "の".len();
            self.tokens.push(Token::new(
                TokenKind::AccessParticle,
                Span::new(no_start, no_end),
            ));

            // 「の」の後の部分を処理
            let after = &word[no_byte_pos + "の".len()..];
            if !after.is_empty() {
                self.emit_word_with_particles(after, no_end);
            }
            return;
        }

        // 「の」が含まれない場合：助詞分離を試みる
        self.emit_word_with_particles(&word, start);
    }

    /// ワードから末尾の助詞を分離してトークンを出力する
    fn emit_word_with_particles(&mut self, word: &str, byte_offset: usize) {
        let word_end = byte_offset + word.len();

        // キーワード判定
        if let Some(kind) = keyword_from_str(word) {
            self.tokens
                .push(Token::new(kind, Span::new(byte_offset, word_end)));
            return;
        }

        // 助詞単体判定
        if let Some(p) = standalone_particle(word) {
            self.tokens.push(Token::new(
                TokenKind::Particle(p),
                Span::new(byte_offset, word_end),
            ));
            return;
        }

        // 「の」単体
        if word == "の" {
            self.tokens.push(Token::new(
                TokenKind::AccessParticle,
                Span::new(byte_offset, word_end),
            ));
            return;
        }

        // 末尾助詞の分離
        if let Some((particle, ident_byte_len)) = Particle::from_suffix(word) {
            let ident_part = &word[..ident_byte_len];
            let ident_span = Span::new(byte_offset, byte_offset + ident_byte_len);
            let particle_span = Span::new(byte_offset + ident_byte_len, word_end);

            if let Some(kind) = keyword_from_str(ident_part) {
                self.tokens.push(Token::new(kind, ident_span));
            } else {
                self.tokens.push(Token::new(
                    TokenKind::Identifier(ident_part.to_string()),
                    ident_span,
                ));
            }
            self.tokens
                .push(Token::new(TokenKind::Particle(particle), particle_span));
            return;
        }

        // 末尾「の」の分離
        if word.ends_with("の") {
            let ident_byte_len = word.len() - "の".len();
            if ident_byte_len > 0 {
                let ident_part = &word[..ident_byte_len];
                let ident_span = Span::new(byte_offset, byte_offset + ident_byte_len);
                let access_span = Span::new(byte_offset + ident_byte_len, word_end);

                if let Some(kind) = keyword_from_str(ident_part) {
                    self.tokens.push(Token::new(kind, ident_span));
                } else {
                    self.tokens.push(Token::new(
                        TokenKind::Identifier(ident_part.to_string()),
                        ident_span,
                    ));
                }
                self.tokens
                    .push(Token::new(TokenKind::AccessParticle, access_span));
                return;
            }
        }

        // 通常の識別子
        self.tokens.push(Token::new(
            TokenKind::Identifier(word.to_string()),
            Span::new(byte_offset, word_end),
        ));
    }

    fn starts_with(&self, s: &str) -> bool {
        self.source[self.pos..].starts_with(s)
    }
}

// === ヘルパー関数 ===

/// 全角数字を半角に変換
fn to_halfwidth_digit(ch: char) -> Option<char> {
    match ch {
        '0'..='9' => Some(ch),
        '０'..='９' => Some(char::from(b'0' + (ch as u32 - '０' as u32) as u8)),
        _ => None,
    }
}

/// 数値の先頭になり得る文字か
fn is_digit_start(ch: char) -> bool {
    ch.is_ascii_digit() || ('０'..='９').contains(&ch)
}

/// 識別子の先頭になり得る文字か
fn is_ident_start(ch: char) -> bool {
    matches!(ch,
        'a'..='z' | 'A'..='Z' | '_'
    ) || is_japanese_char(ch)
}

/// 識別子の継続文字か
#[allow(dead_code)]
fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit() || ('０'..='９').contains(&ch)
}

/// 日本語文字か（ひらがな・カタカナ・漢字・長音）
fn is_japanese_char(ch: char) -> bool {
    matches!(ch,
        '\u{3040}'..='\u{309F}' |  // ひらがな
        '\u{30A0}'..='\u{30FF}' |  // カタカナ（長音符ーを含む）
        '\u{4E00}'..='\u{9FFF}' |  // CJK統合漢字
        '\u{3400}'..='\u{4DBF}' |  // CJK統合漢字拡張A
        '\u{F900}'..='\u{FAFF}'    // CJK互換漢字
    )
}

/// 助数詞に使える文字か（漢字・カタカナのみ、ひらがなは除外）
fn is_counter_char(ch: char) -> bool {
    matches!(ch,
        '\u{30A0}'..='\u{30FF}' |  // カタカナ（長音符ーを含む）
        '\u{4E00}'..='\u{9FFF}' |  // CJK統合漢字
        '\u{3400}'..='\u{4DBF}' |  // CJK統合漢字拡張A
        '\u{F900}'..='\u{FAFF}'    // CJK互換漢字
    )
}

/// 助詞単体の文字列を判定
fn standalone_particle(word: &str) -> Option<Particle> {
    match word {
        "を" => Some(Particle::Wo),
        "が" => Some(Particle::Ga),
        "に" => Some(Particle::Ni),
        "で" => Some(Particle::De),
        "から" => Some(Particle::Kara),
        "まで" => Some(Particle::Made),
        "と" => Some(Particle::To),
        "へ" => Some(Particle::He),
        "より" => Some(Particle::Yori),
        _ => None,
    }
}

/// キーワード判定
fn keyword_from_str(word: &str) -> Option<TokenKind> {
    match word {
        "は" => Some(TokenKind::Ha),
        "という" => Some(TokenKind::ToIu),
        "手順" => Some(TokenKind::Tejun),
        "組" => Some(TokenKind::Kumi),
        "特性" => Some(TokenKind::Tokusei),
        "もし" => Some(TokenKind::Moshi),
        "ならば" => Some(TokenKind::Naraba),
        "もしくは" => Some(TokenKind::Moshikuha),
        "そうでなければ" => Some(TokenKind::SouDenakereba),
        "繰り返す" => Some(TokenKind::KuriKaesu),
        "間" => Some(TokenKind::Aida),
        "回" => Some(TokenKind::Kai),
        "それぞれについて" => Some(TokenKind::SorezoreNiTsuite),
        "返す" => Some(TokenKind::Kaesu),
        "変わる" => Some(TokenKind::Kawaru),
        "変える" => Some(TokenKind::Kaeru),
        "試す" => Some(TokenKind::Tamesu),
        "失敗した場合" => Some(TokenKind::ShippaiShitaBaai),
        "必ず行う" => Some(TokenKind::KanarazuOkonau),
        "訴える" => Some(TokenKind::Uttaeru),
        "使う" => Some(TokenKind::Tsukau),
        "公開" => Some(TokenKind::Koukai),
        "作る" => Some(TokenKind::Tsukuru),
        "持つ" => Some(TokenKind::Motsu),
        "どれかを調べる" => Some(TokenKind::DorekaWoShiraberu),
        "場合" => Some(TokenKind::NoBaai),
        "どれでもない場合" => Some(TokenKind::DoreDemoNaiBaai),
        "どれでもない" => Some(TokenKind::DoreDemoNai),
        "どれか" => Some(TokenKind::Doreka),
        "次へ" => Some(TokenKind::TsugiHe),
        "抜ける" => Some(TokenKind::Nukeru),
        "分岐して" => Some(TokenKind::BunkiShite),
        "かつ" => Some(TokenKind::Katsu),
        "または" => Some(TokenKind::Mataha),
        "でない" => Some(TokenKind::DeNai),
        "表示する" => Some(TokenKind::HyoujiSuru),
        "これ" => Some(TokenKind::Kore),
        "それ" => Some(TokenKind::Sore),
        "あれ" => Some(TokenKind::Are),
        "こう" => Some(TokenKind::Kou),
        "ここ" => Some(TokenKind::Koko),
        "そこ" => Some(TokenKind::Soko),
        "しながら" => Some(TokenKind::Shinagara),
        "待つ" => Some(TokenKind::Matsu),
        "背景で" => Some(TokenKind::Haikeide),
        "真" => Some(TokenKind::Bool(true)),
        "偽" => Some(TokenKind::Bool(false)),
        _ => None,
    }
}
