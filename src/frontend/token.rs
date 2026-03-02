use crate::common::source::Span;

/// トークンの種類
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // === リテラル ===
    /// 整数リテラル（文字列として保持、BigInt変換はパーサ側）
    Integer(String),
    /// 小数リテラル
    Float(String),
    /// 文字列リテラル（式展開を含む場合は StringParts に分割される）
    String(String),
    /// 文字列中の式展開の開始 【
    StringInterpStart,
    /// 文字列中の式展開の終了 】
    StringInterpEnd,
    /// 真偽リテラル
    Bool(bool),

    // === 識別子 ===
    /// 一般の識別子（変数名、手順名など）
    Identifier(String),

    // === 助詞 ===
    /// 役割助詞トークン（を/が/に/で/から/まで/と/へ/より）
    Particle(Particle),
    /// アクセス助詞「の」（属性アクセス・スコープ参照）
    AccessParticle,

    // === 助数詞 ===
    /// 助数詞（数値の直後に付く単位的な文字列）
    Counter(String),

    // === キーワード ===
    /// は（束縛）
    Ha,
    /// という
    ToIu,
    /// 手順
    Tejun,
    /// 組
    Kumi,
    /// 特性
    Tokusei,
    /// もし
    Moshi,
    /// ならば
    Naraba,
    /// もしくは
    Moshikuha,
    /// そうでなければ
    SouDenakereba,
    /// 繰り返す
    KuriKaesu,
    /// 間
    Aida,
    /// 回
    Kai,
    /// それぞれについて
    SorezoreNiTsuite,
    /// 返す
    Kaesu,
    /// 変わる
    Kawaru,
    /// 変える
    Kaeru,
    /// 試す
    Tamesu,
    /// 失敗した場合
    ShippaiShitaBaai,
    /// 必ず行う
    KanarazuOkonau,
    /// 訴える
    Uttaeru,
    /// 使う
    Tsukau,
    /// 公開
    Koukai,
    /// 作る
    Tsukuru,
    /// 持つ
    Motsu,
    /// どれかを調べる
    DorekaWoShiraberu,
    /// の場合
    NoBaai,
    /// どれでもない場合
    DoreDemoNaiBaai,
    /// どれでもない
    DoreDemoNai,
    /// どれか
    Doreka,
    /// 次へ
    TsugiHe,
    /// 抜ける
    Nukeru,
    /// 分岐して
    BunkiShite,
    /// かつ
    Katsu,
    /// または
    Mataha,
    /// でない
    DeNai,
    /// 表示する
    HyoujiSuru,
    /// 入力する
    NyuuryokuSuru,
    /// これ
    Kore,
    /// それ
    Sore,
    /// あれ
    Are,
    /// こう
    Kou,
    /// ここ
    Koko,
    /// そこ
    Soko,
    /// しながら（将来予約）
    Shinagara,
    /// 待つ（将来予約）
    Matsu,
    /// 背景で（将来予約）
    Haikeide,

    // === 記号 ===
    /// 【
    LBracket,
    /// 】
    RBracket,
    /// ｛
    LBrace,
    /// ｝
    RBrace,
    /// （
    LParen,
    /// ）
    RParen,
    /// 、
    Comma,
    /// 。
    Period,
    /// :
    Colon,
    /// …
    Ellipsis,

    // === 構造 ===
    /// インデント増加
    Indent,
    /// インデント減少
    Dedent,
    /// 改行（文の終端）
    Newline,
    /// ファイル終端
    Eof,

    // === コメント ===
    /// 行コメント（※ ...）
    LineComment(String),
    /// ブロックコメント（（...）
    BlockComment(String),

    // === エラー ===
    /// 不明なトークン
    Error(String),
}

/// 助詞の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Particle {
    /// を — 対象（目的語）
    Wo,
    /// が — 主語・主格
    Ga,
    /// に — 目標・宛先
    Ni,
    /// で — 手段・方法
    De,
    /// から — 起点
    Kara,
    /// まで — 終点
    Made,
    /// と — 共同・並列
    To,
    /// へ — 方向
    He,
    /// より — 比較基準
    Yori,
}

impl Particle {
    /// 文字列を助詞に変換する（識別子末尾からの分離用）
    pub fn from_suffix(s: &str) -> Option<(Particle, usize)> {
        // 最長一致で助詞を分離する
        // 長い助詞から順にチェック
        let suffixes: &[(&str, Particle)] = &[
            ("から", Particle::Kara),
            ("まで", Particle::Made),
            ("より", Particle::Yori),
            ("を", Particle::Wo),
            ("が", Particle::Ga),
            ("に", Particle::Ni),
            ("で", Particle::De),
            ("と", Particle::To),
            ("へ", Particle::He),
        ];
        for &(suffix, particle) in suffixes {
            if s.ends_with(suffix) {
                let ident_len = s.len() - suffix.len();
                if ident_len > 0 {
                    return Some((particle, ident_len));
                }
            }
        }
        None
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Particle::Wo => "を",
            Particle::Ga => "が",
            Particle::Ni => "に",
            Particle::De => "で",
            Particle::Kara => "から",
            Particle::Made => "まで",
            Particle::To => "と",
            Particle::He => "へ",
            Particle::Yori => "より",
        }
    }
}

impl std::fmt::Display for Particle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 位置情報付きトークン
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Integer(n) => write!(f, "整数({n})"),
            TokenKind::Float(n) => write!(f, "小数({n})"),
            TokenKind::String(s) => write!(f, "文字列「{s}」"),
            TokenKind::Bool(b) => {
                write!(f, "{}", if *b { "真" } else { "偽" })
            }
            TokenKind::Identifier(s) => write!(f, "識別子({s})"),
            TokenKind::Particle(p) => write!(f, "助詞({p})"),
            TokenKind::AccessParticle => write!(f, "助詞(の)"),
            TokenKind::Counter(c) => write!(f, "助数詞({c})"),
            TokenKind::Newline => write!(f, "改行"),
            TokenKind::Indent => write!(f, "字下げ"),
            TokenKind::Dedent => write!(f, "字戻し"),
            TokenKind::Eof => write!(f, "終端"),
            TokenKind::Error(msg) => write!(f, "エラー({msg})"),
            _ => write!(f, "{:?}", self),
        }
    }
}
