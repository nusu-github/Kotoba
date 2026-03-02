use crate::common::source::Span;
use crate::frontend::token::Particle;

/// AST のルートノード: プログラム全体
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

/// 文ノード
#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    /// 束縛: `名前 は 式`
    Bind {
        name: String,
        mutable: bool,
        value: Expr,
    },

    /// 再束縛: `名前を 式 に変える`
    Rebind { name: String, value: Expr },

    /// 式文
    ExprStmt(Expr),

    /// 手順定義: `挨拶する という手順【名前:を】 ...`
    ProcDef {
        name: String,
        params: Vec<Param>,
        return_type: Option<String>,
        body: Block,
        is_public: bool,
    },

    /// 組定義: `人 という組 ...`
    StructDef {
        name: String,
        fields: Vec<FieldDef>,
        methods: Vec<Stmt>, // ProcDef のみ
        is_public: bool,
    },

    /// 特性定義: `表示できる という特性 ...`
    TraitDef {
        name: String,
        methods: Vec<Stmt>,
        is_public: bool,
    },

    /// 特性実装: `人 は 表示できる を持つ ...`
    TraitImpl {
        type_name: String,
        trait_name: String,
        methods: Vec<Stmt>,
    },

    /// モジュール読み込み: `「ファイル操作」を 使う`
    Use {
        module: String,
        items: Option<Vec<String>>,
    },

    /// 返す
    Return(Option<Expr>),

    /// 次へ (continue)
    Continue,

    /// 抜ける (break)
    Break,
}

/// 引数宣言
#[derive(Debug, Clone)]
pub struct Param {
    pub name: Option<String>, // None の場合は短縮形（`:を` → 助詞がそのまま名前）
    pub particle: Particle,
    pub span: Span,
}

/// フィールド定義（組のフィールド）
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub type_name: Option<String>,
    pub span: Span,
}

/// ブロック（インデントされた文の集合）
#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

/// 式ノード
#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    /// 整数リテラル
    Integer(String),

    /// 小数リテラル
    Float(String),

    /// 文字列リテラル
    StringLiteral(String),

    /// 式展開を含む文字列
    StringInterp(Vec<StringPart>),

    /// 真偽リテラル
    Bool(bool),

    /// 無
    None,

    /// 変数参照
    Identifier(String),

    /// こそあど参照
    KosoAdo(KosoAdoKind),

    /// 一覧リテラル: 【1、2、3】
    List(Vec<Expr>),

    /// 対応表リテラル: ｛名前:「太郎」、年齢: 25｝
    Map(Vec<(String, Expr)>),

    /// 助詞式（手順呼び出し）: `(式 助詞)+ 動詞`
    Call {
        callee: String,
        args: Vec<ParticleArg>,
    },

    /// 属性アクセス: `式の名前`
    PropertyAccess { object: Box<Expr>, property: String },

    /// メソッド呼び出し: `対象の動詞する`
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<ParticleArg>,
    },

    /// 算術演算: `aとbの和`
    BinaryOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// 単項演算: `aが真 でない`
    UnaryOp { op: UnaryOp, operand: Box<Expr> },

    /// 比較: `aが bより大きい`
    Comparison {
        op: CompOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// 論理演算: `a かつ b`, `a または b`
    Logical {
        op: LogicalOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// 条件分岐（式として）
    If {
        condition: Box<Expr>,
        then_block: Block,
        elif_clauses: Vec<(Expr, Block)>,
        else_block: Option<Block>,
    },

    /// パターンマッチ（式として）
    Match {
        target: Box<Expr>,
        arms: Vec<MatchArm>,
    },

    /// 繰り返し
    Loop(Box<LoopKind>),

    /// て形チェイン（パイプライン）
    TeChain { steps: Vec<ChainStep> },

    /// 分岐して（チェイン途中の分岐）
    BranchChain { if_expr: Box<Expr> },

    /// 無名手順（ラムダ）: `（【x:を】xとxの積を返す）`
    Lambda { params: Vec<Param>, body: Block },

    /// 試す〜失敗した場合〜必ず行う
    TryCatch {
        body: Block,
        catch_param: Option<String>,
        catch_body: Option<Block>,
        finally_body: Option<Block>,
    },

    /// 訴える（throw）
    Throw(Box<Expr>),

    /// 助数詞付き数値: `5秒`
    WithCounter { value: Box<Expr>, counter: String },

    /// 組のインスタンス生成: `人を作る【名前:「太郎」、年齢: 25】`
    Construct {
        type_name: String,
        fields: Vec<(String, Expr)>,
    },

    /// 分解束縛: `【先頭、残り…】は ...`
    Destructure {
        pattern: DestructurePattern,
        value: Box<Expr>,
    },
}

/// 文字列の部品（式展開用）
#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Expr(Expr),
}

/// こそあど指示語の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KosoAdoKind {
    Kore, // これ
    Sore, // それ
    Are,  // あれ
    Kou,  // こう（再帰用）
    Koko, // ここ
    Soko, // そこ
}

/// 助詞付き引数
#[derive(Debug, Clone)]
pub struct ParticleArg {
    pub value: Expr,
    pub particle: Particle,
    pub span: Span,
}

/// 二項演算子
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, // 和
    Sub, // 差
    Mul, // 積
    Div, // 割る
    Mod, // 割った余り
}

/// 単項演算子
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not, // でない
}

/// 比較演算子
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompOp {
    Gt, // より大きい
    Lt, // より小さい
    Eq, // と等しい
    Ne, // と等しくない
    Ge, // 以上
    Le, // 以下
}

/// 論理演算子
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And, // かつ
    Or,  // または
}

/// パターンマッチの腕
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
    pub span: Span,
}

/// パターン
#[derive(Debug, Clone)]
pub enum Pattern {
    /// リテラル値
    Literal(Expr),
    /// 変数束縛
    Binding(String),
    /// ワイルドカード（どれか）
    Wildcard,
    /// リスト分解 【名前、年齢、どれか】
    List(Vec<Pattern>),
    /// どれでもない場合（デフォルト）
    Default,
}

/// 繰り返しの種類
#[derive(Debug, Clone)]
pub enum LoopKind {
    /// N回繰り返す【i】
    Times {
        count: Expr,
        var: Option<String>,
        body: Block,
    },
    /// AからBまで繰り返す【i】
    Range {
        from: Expr,
        to: Expr,
        var: Option<String>,
        body: Block,
    },
    /// 条件 間 繰り返す
    While { condition: Expr, body: Block },
    /// 一覧の それぞれについて【要素】
    ForEach {
        iterable: Expr,
        var: String,
        body: Block,
    },
}

/// て形チェインのステップ
#[derive(Debug, Clone)]
pub enum ChainStep {
    /// 通常の手順呼び出し（て形 or 終止形）
    Call {
        callee: String,
        args: Vec<ParticleArg>,
    },
    /// 分岐して
    Branch { if_expr: Expr },
}

/// 分解束縛パターン
#[derive(Debug, Clone)]
pub enum DestructurePattern {
    /// 【先頭、残り…】
    List {
        elements: Vec<String>,
        rest: Option<String>,
    },
    /// ｛名前、年齢｝
    Map { keys: Vec<String> },
}
