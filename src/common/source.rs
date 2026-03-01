use unicode_normalization::UnicodeNormalization;

/// ソース位置情報
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// ソースコード中のバイトオフセット（開始）
    pub start: usize,
    /// ソースコード中のバイトオフセット（終了、排他的）
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// 2つのSpanを結合して、両方を含む最小のSpanを返す
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// 行と列の位置（人間向け表示用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineCol {
    pub line: usize,   // 1始まり
    pub column: usize, // 1始まり
}

/// ソースコード全体を保持し、位置変換などを行う
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub name: String,
    pub content: String,
    /// 各行の開始バイトオフセット
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        let content = normalize_for_lexing(content.into().as_str());
        let mut line_starts = vec![0];
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            name: name.into(),
            content,
            line_starts,
        }
    }

    /// バイトオフセットから行・列を計算する
    pub fn line_col(&self, offset: usize) -> LineCol {
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let col = offset - self.line_starts[line];
        LineCol {
            line: line + 1,
            column: col + 1,
        }
    }

    /// Span に対応するソーステキストを取得する
    pub fn slice(&self, span: Span) -> &str {
        &self.content[span.start..span.end]
    }
}

fn normalize_for_lexing(input: &str) -> String {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Normal,
        StringLiteral,
        LineComment,
        BlockComment { depth: usize },
    }

    let mut mode = Mode::Normal;
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut normal_start = 0usize;

    while i < input.len() {
        match mode {
            Mode::Normal => {
                if input[i..].starts_with("※（") {
                    if normal_start < i {
                        out.push_str(&input[normal_start..i].nfkc().collect::<String>());
                    }
                    out.push_str("※（");
                    i += "※（".len();
                    mode = Mode::BlockComment { depth: 1 };
                    continue;
                }

                let ch = input[i..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary while normalizing");
                if ch == '※' {
                    if normal_start < i {
                        out.push_str(&input[normal_start..i].nfkc().collect::<String>());
                    }
                    out.push(ch);
                    i += ch.len_utf8();
                    mode = Mode::LineComment;
                    continue;
                }
                if ch == '「' {
                    if normal_start < i {
                        out.push_str(&input[normal_start..i].nfkc().collect::<String>());
                    }
                    out.push(ch);
                    i += ch.len_utf8();
                    mode = Mode::StringLiteral;
                    continue;
                }
                i += ch.len_utf8();
            }
            Mode::StringLiteral => {
                let ch = input[i..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary while normalizing");
                out.push(ch);
                i += ch.len_utf8();
                if ch == '」' {
                    mode = Mode::Normal;
                    normal_start = i;
                }
            }
            Mode::LineComment => {
                let ch = input[i..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary while normalizing");
                out.push(ch);
                i += ch.len_utf8();
                if ch == '\n' {
                    mode = Mode::Normal;
                    normal_start = i;
                }
            }
            Mode::BlockComment { depth } => {
                if input[i..].starts_with("※（") {
                    out.push_str("※（");
                    i += "※（".len();
                    mode = Mode::BlockComment { depth: depth + 1 };
                    continue;
                }
                if input[i..].starts_with("）※") {
                    out.push_str("）※");
                    i += "）※".len();
                    if depth == 1 {
                        mode = Mode::Normal;
                        normal_start = i;
                    } else {
                        mode = Mode::BlockComment { depth: depth - 1 };
                    }
                    continue;
                }
                let ch = input[i..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary while normalizing");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }

    if mode == Mode::Normal && normal_start < input.len() {
        out.push_str(&input[normal_start..].nfkc().collect::<String>());
    }

    out
}
