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
        let content = content.into();
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
