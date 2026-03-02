use num_bigint::BigInt;
use std::collections::HashMap;
use std::fmt;

/// VM の値型
#[derive(Debug, Clone)]
pub enum Value {
    /// 整数（任意精度）
    Integer(BigInt),
    /// 小数（f64）
    Float(f64),
    /// 文字列
    String(String),
    /// 真偽値
    Bool(bool),
    /// 一覧
    List(Vec<Value>),
    /// 対応表
    Map(HashMap<String, Value>),
    /// 手順（関数参照）
    Procedure(ProcRef),
    /// 無
    None,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "数（整数）",
            Value::Float(_) => "数（小数）",
            Value::String(_) => "文字列",
            Value::Bool(_) => "真偽",
            Value::List(_) => "一覧",
            Value::Map(_) => "対応表",
            Value::Procedure(_) => "手順",
            Value::None => "無",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Integer(n) => *n != BigInt::from(0),
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Map(m) => !m.is_empty(),
            Value::Procedure(_) => true,
            Value::None => false,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Value::Integer(n) => n.to_string(),
            Value::Float(f) => format!("{f}"),
            Value::String(s) => s.clone(),
            Value::Bool(true) => "真".into(),
            Value::Bool(false) => "偽".into(),
            Value::List(items) => {
                let inner: Vec<String> = items.iter().map(|v| v.to_display_string()).collect();
                format!("【{}】", inner.join("、"))
            }
            Value::Map(map) => {
                let inner: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}:{}", k, v.to_display_string()))
                    .collect();
                format!("｛{}｝", inner.join("、"))
            }
            Value::Procedure(p) => format!("<手順 {}>", p.name),
            Value::None => "無".into(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Integer(a), Value::Float(b)) => {
                // 整数と小数の比較
                *b == a.to_string().parse::<f64>().unwrap_or(f64::NAN)
            }
            (Value::Float(a), Value::Integer(b)) => {
                *a == b.to_string().parse::<f64>().unwrap_or(f64::NAN)
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::None, Value::None) => true,
            (Value::List(a), Value::List(b)) => a == b,
            _ => false,
        }
    }
}

/// 手順の参照
#[derive(Debug, Clone)]
pub struct ProcRef {
    pub name: String,
    /// バイトコードチャンク内のオフセット
    pub chunk_id: usize,
    /// パラメータ数
    pub arity: usize,
}

/// バイトコード命令
#[derive(Debug, Clone)]
pub enum OpCode {
    /// 定数をスタックに積む
    Constant(usize),

    /// `無` をスタックに積む
    PushNone,
    /// `真` をスタックに積む
    PushTrue,
    /// `偽` をスタックに積む
    PushFalse,

    /// ローカル変数を読み込む
    LoadLocal(usize),
    /// 現在実行中の手順を読み込む
    LoadCurrentProc,
    /// ローカル変数に書き込む
    StoreLocal(usize),
    /// グローバル変数を読み込む
    LoadGlobal(String),
    /// グローバル変数に書き込む
    StoreGlobal(String),

    // === 算術演算 ===
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Negate,

    // === 比較演算 ===
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,

    // === 論理演算 ===
    Not,
    And,
    Or,

    // === ジャンプ ===
    /// 無条件ジャンプ
    Jump(usize),
    /// スタックトップが偽なら ジャンプ
    JumpIfFalse(usize),
    /// スタックトップが真ならジャンプ
    JumpIfTrue(usize),

    // === 関数呼び出し ===
    /// 引数の数を指定して呼ぶ
    Call(usize),
    /// 値を返す
    Return,

    // === スタック操作 ===
    Pop,
    Dup,

    // === 一覧/対応表操作 ===
    /// N個の要素から一覧を構築
    BuildList(usize),
    /// N個のキー・値ペアから対応表を構築
    BuildMap(usize),
    /// インデックスアクセス（1始まり）
    Index,
    /// プロパティアクセス
    GetProperty(String),
    /// プロパティ設定
    SetProperty(String),

    // === 文字列操作 ===
    /// N個の部品から文字列を連結
    Concat(usize),

    // === 入出力 ===
    /// 表示する（スタックトップを出力）
    Print,
    /// 標準入力から1行読み込む
    ReadInput,
    /// ファイルを読み込む
    ReadFile,
    /// ファイルへ書き込む
    WriteFile,

    // === 将来の非同期対応用予約 ===
    /// 中断ポイント（v0.x では使用しない）
    _Suspend,
    /// 再開ポイント（v0.x では使用しない）
    _Resume,

    // === 例外処理 ===
    /// try ブロック開始（catch ハンドラのアドレスを指定）
    SetupTry(usize),
    /// try ブロック正常終了（try フレームを除去）
    EndTry,
    /// 例外を投げる（スタックトップが例外値）
    Throw,

    /// プログラム終了
    Halt,
}

/// バイトコードチャンク（一つの手順 or トップレベル）
#[derive(Debug, Clone)]
pub struct Chunk {
    pub name: String,
    pub code: Vec<OpCode>,
    pub constants: Vec<Value>,
    /// パラメータ数
    pub arity: usize,
    /// ローカル変数の数
    pub local_count: usize,
}

impl Chunk {
    pub fn new(name: impl Into<String>, arity: usize) -> Self {
        Self {
            name: name.into(),
            code: Vec::new(),
            constants: Vec::new(),
            arity,
            local_count: 0,
        }
    }

    /// 定数を追加し、そのインデックスを返す
    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.push(value);
        self.constants.len() - 1
    }

    /// 命令を追加し、そのインデックスを返す
    pub fn emit(&mut self, op: OpCode) -> usize {
        self.code.push(op);
        self.code.len() - 1
    }

    /// 現在のコード位置を返す
    pub fn current_pos(&self) -> usize {
        self.code.len()
    }

    /// ジャンプ命令のターゲットを修正する
    pub fn patch_jump(&mut self, pos: usize, target: usize) {
        match &mut self.code[pos] {
            OpCode::Jump(t)
            | OpCode::JumpIfFalse(t)
            | OpCode::JumpIfTrue(t)
            | OpCode::SetupTry(t) => {
                *t = target;
            }
            _ => panic!("ジャンプ命令以外をパッチしようとしました"),
        }
    }

    /// デバッグ用: バイトコードのダンプ
    pub fn disassemble(&self) -> String {
        let mut out = format!(
            "=== {} (引数:{}, 局所変数:{}) ===\n",
            self.name, self.arity, self.local_count
        );
        for (i, op) in self.code.iter().enumerate() {
            out.push_str(&format!("{:04}  {:?}\n", i, op));
        }
        out
    }
}
