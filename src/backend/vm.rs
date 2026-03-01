use std::collections::HashMap;

use num_bigint::BigInt;
use num_traits::ToPrimitive;

use crate::bytecode::{Chunk, OpCode, Value};

/// VM 実行時エラー
#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "実行時エラー: {}", self.message)
    }
}

/// コールフレーム
#[derive(Debug)]
struct CallFrame {
    /// 実行中のチャンクのインデックス
    chunk_id: usize,
    /// 命令ポインタ
    ip: usize,
    /// スタック上のフレームベース（ローカル変数のスタック開始位置）
    base: usize,
}

/// try フレーム（例外ハンドラ情報）
#[derive(Debug)]
struct TryFrame {
    /// catch ハンドラのジャンプ先
    catch_target: usize,
    /// try 開始時のスタック深さ
    stack_depth: usize,
    /// try 開始時のチャンクID
    chunk_id: usize,
}

/// スタックベースの仮想マシン
pub struct VM {
    /// 全チャンク
    chunks: Vec<Chunk>,
    /// 値スタック
    stack: Vec<Value>,
    /// コールフレームスタック
    frames: Vec<CallFrame>,
    /// グローバル変数
    globals: HashMap<String, Value>,
    /// try フレームスタック
    try_stack: Vec<TryFrame>,
    /// 出力バッファ（テスト用）
    pub output: Vec<String>,
}

impl VM {
    pub fn new(chunks: Vec<Chunk>) -> Self {
        Self {
            chunks,
            stack: Vec::with_capacity(256),
            frames: Vec::new(),
            globals: HashMap::new(),
            try_stack: Vec::new(),
            output: Vec::new(),
        }
    }

    /// プログラムを実行する
    pub fn run(&mut self) -> Result<Value, RuntimeError> {
        // メインチャンク（chunks[0]）から開始
        self.frames.push(CallFrame {
            chunk_id: 0,
            ip: 0,
            base: 0,
        });

        // ローカル変数用のスロットを確保
        let local_count = self.chunks[0].local_count;
        for _ in 0..local_count {
            self.stack.push(Value::None);
        }

        self.execute()
    }

    fn execute(&mut self) -> Result<Value, RuntimeError> {
        loop {
            let frame = self.frames.last().ok_or_else(|| RuntimeError {
                message: "フレームスタックが空です".into(),
            })?;

            let chunk_id = frame.chunk_id;
            let ip = frame.ip;
            let base = frame.base;

            if ip >= self.chunks[chunk_id].code.len() {
                return Ok(Value::None);
            }

            let op = self.chunks[chunk_id].code[ip].clone();

            // 命令ポインタを進める
            if let Some(frame) = self.frames.last_mut() {
                frame.ip += 1;
            }

            match op {
                OpCode::Constant(idx) => {
                    let value = self.chunks[chunk_id].constants[idx].clone();
                    self.stack.push(value);
                }

                OpCode::PushNone => self.stack.push(Value::None),
                OpCode::PushTrue => self.stack.push(Value::Bool(true)),
                OpCode::PushFalse => self.stack.push(Value::Bool(false)),

                OpCode::LoadLocal(idx) => {
                    let value = self.stack[base + idx].clone();
                    self.stack.push(value);
                }

                OpCode::StoreLocal(idx) => {
                    let value = self.stack.last().cloned().unwrap_or(Value::None);
                    let slot = base + idx;
                    if slot >= self.stack.len() {
                        self.stack.resize(slot + 1, Value::None);
                    }
                    self.stack[slot] = value;
                }

                OpCode::LoadGlobal(name) => {
                    let value = self.globals.get(&name).cloned().unwrap_or(Value::None);
                    self.stack.push(value);
                }

                OpCode::StoreGlobal(name) => {
                    let value = self.stack.last().cloned().unwrap_or(Value::None);
                    self.globals.insert(name, value);
                }

                OpCode::Pop => {
                    self.stack.pop();
                }

                OpCode::Dup => {
                    let value = self.stack.last().cloned().unwrap_or(Value::None);
                    self.stack.push(value);
                }

                // === 算術 ===
                OpCode::Add => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let result = self.arithmetic_add(&a, &b)?;
                    self.stack.push(result);
                }

                OpCode::Sub => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let result = self.arithmetic_sub(&a, &b)?;
                    self.stack.push(result);
                }

                OpCode::Mul => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let result = self.arithmetic_mul(&a, &b)?;
                    self.stack.push(result);
                }

                OpCode::Div => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let result = self.arithmetic_div(&a, &b)?;
                    self.stack.push(result);
                }

                OpCode::Mod => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let result = self.arithmetic_mod(&a, &b)?;
                    self.stack.push(result);
                }

                OpCode::Negate => {
                    let a = self.pop_value()?;
                    let result = match a {
                        Value::Integer(n) => Value::Integer(-n),
                        Value::Float(f) => Value::Float(-f),
                        _ => {
                            return Err(RuntimeError {
                                message: format!("{}は符号反転できません", a.type_name()),
                            })
                        }
                    };
                    self.stack.push(result);
                }

                // === 比較 ===
                OpCode::Equal => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(a == b));
                }

                OpCode::NotEqual => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(a != b));
                }

                OpCode::Greater => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(self.compare_gt(&a, &b)?));
                }

                OpCode::Less => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(self.compare_lt(&a, &b)?));
                }

                OpCode::GreaterEqual => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let gt = self.compare_gt(&a, &b)?;
                    self.stack.push(Value::Bool(gt || a == b));
                }

                OpCode::LessEqual => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    let lt = self.compare_lt(&a, &b)?;
                    self.stack.push(Value::Bool(lt || a == b));
                }

                // === 論理 ===
                OpCode::Not => {
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(!a.is_truthy()));
                }

                OpCode::And => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(a.is_truthy() && b.is_truthy()));
                }

                OpCode::Or => {
                    let b = self.pop_value()?;
                    let a = self.pop_value()?;
                    self.stack.push(Value::Bool(a.is_truthy() || b.is_truthy()));
                }

                // === ジャンプ ===
                OpCode::Jump(target) => {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.ip = target;
                    }
                }

                OpCode::JumpIfFalse(target) => {
                    let value = self.stack.last().cloned().unwrap_or(Value::None);
                    if !value.is_truthy() {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.ip = target;
                        }
                    }
                }

                OpCode::JumpIfTrue(target) => {
                    let value = self.stack.last().cloned().unwrap_or(Value::None);
                    if value.is_truthy() {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.ip = target;
                        }
                    }
                }

                // === 手順呼び出し ===
                OpCode::Call(arity) => {
                    // スタック: [... callee, arg1, arg2, ...]
                    let callee_idx = self.stack.len() - arity - 1;
                    let callee = self.stack[callee_idx].clone();

                    match callee {
                        Value::Procedure(proc_ref) => {
                            if proc_ref.arity != arity {
                                return Err(RuntimeError {
                                    message: format!(
                                        "手順「{}」は{}個の引数が必要ですが、{}個渡されました",
                                        proc_ref.name, proc_ref.arity, arity
                                    ),
                                });
                            }

                            let new_base = callee_idx + 1; // 引数の開始位置

                            // ローカル変数用のスロットを確保
                            let local_count = self.chunks[proc_ref.chunk_id].local_count;
                            let needed = new_base + local_count;
                            while self.stack.len() < needed {
                                self.stack.push(Value::None);
                            }

                            self.frames.push(CallFrame {
                                chunk_id: proc_ref.chunk_id,
                                ip: 0,
                                base: new_base,
                            });
                        }
                        _ => {
                            return Err(RuntimeError {
                                message: format!(
                                    "「{}」は呼び出し可能ではありません",
                                    callee.type_name()
                                ),
                            });
                        }
                    }
                }

                OpCode::Return => {
                    let result = self.stack.pop().unwrap_or(Value::None);

                    // 現在のフレームを破棄
                    let finished_frame = self.frames.pop().ok_or_else(|| RuntimeError {
                        message: "フレームスタックが空です".into(),
                    })?;

                    if self.frames.is_empty() {
                        // メインの終了
                        return Ok(result);
                    }

                    // callee と引数をスタックから除去
                    // callee はbase - 1の位置にある
                    let callee_pos = if finished_frame.base > 0 {
                        finished_frame.base - 1
                    } else {
                        0
                    };
                    self.stack.truncate(callee_pos);
                    self.stack.push(result);
                }

                // === 一覧/対応表 ===
                OpCode::BuildList(count) => {
                    let start = self.stack.len() - count;
                    let elements: Vec<Value> = self.stack.drain(start..).collect();
                    self.stack.push(Value::List(elements));
                }

                OpCode::BuildMap(count) => {
                    let start = self.stack.len() - count * 2;
                    let pairs: Vec<Value> = self.stack.drain(start..).collect();
                    let mut map = HashMap::new();
                    for chunk in pairs.chunks(2) {
                        if let (Value::String(key), value) = (&chunk[0], &chunk[1]) {
                            map.insert(key.clone(), value.clone());
                        }
                    }
                    self.stack.push(Value::Map(map));
                }

                OpCode::Index => {
                    let idx = self.pop_value()?;
                    let collection = self.pop_value()?;
                    match (&collection, &idx) {
                        (Value::List(list), Value::Integer(n)) => {
                            // 1始まりインデックス
                            let i = n.to_usize().unwrap_or(0);
                            if i >= 1 && i <= list.len() {
                                self.stack.push(list[i - 1].clone());
                            } else {
                                return Err(RuntimeError {
                                    message: format!(
                                        "インデックス{}は範囲外です（一覧の長さ: {}）",
                                        i,
                                        list.len()
                                    ),
                                });
                            }
                        }
                        _ => {
                            return Err(RuntimeError {
                                message: format!(
                                    "{}に対してインデックスアクセスはできません",
                                    collection.type_name()
                                ),
                            });
                        }
                    }
                }

                OpCode::GetProperty(prop) => {
                    let object = self.pop_value()?;
                    match &object {
                        Value::Map(map) => {
                            let value = map.get(&prop).cloned().unwrap_or(Value::None);
                            self.stack.push(value);
                        }
                        Value::List(list) => match prop.as_str() {
                            "長さ" => {
                                self.stack.push(Value::Integer(BigInt::from(list.len())));
                            }
                            _ => {
                                self.stack.push(Value::None);
                            }
                        },
                        Value::String(s) => match prop.as_str() {
                            "長さ" => {
                                self.stack
                                    .push(Value::Integer(BigInt::from(s.chars().count())));
                            }
                            _ => {
                                self.stack.push(Value::None);
                            }
                        },
                        _ => {
                            self.stack.push(Value::None);
                        }
                    }
                }

                OpCode::SetProperty(prop) => {
                    let value = self.pop_value()?;
                    let mut object = self.pop_value()?;
                    if let Value::Map(ref mut map) = object {
                        map.insert(prop, value);
                    }
                    self.stack.push(object);
                }

                // === 文字列 ===
                OpCode::Concat(count) => {
                    let start = self.stack.len() - count;
                    let parts: Vec<Value> = self.stack.drain(start..).collect();
                    let result: String = parts.iter().map(|v| v.to_display_string()).collect();
                    self.stack.push(Value::String(result));
                }

                // === 入出力 ===
                OpCode::Print => {
                    let value = self.pop_value()?;
                    let text = value.to_display_string();
                    println!("{}", text);
                    self.output.push(text);
                }

                OpCode::Halt => {
                    return Ok(self.stack.pop().unwrap_or(Value::None));
                }

                OpCode::SetupTry(catch_target) => {
                    self.try_stack.push(TryFrame {
                        catch_target,
                        stack_depth: self.stack.len(),
                        chunk_id,
                    });
                }

                OpCode::EndTry => {
                    self.try_stack.pop();
                }

                OpCode::Throw => {
                    let exception = self.pop_value()?;
                    let mut handled = false;

                    while let Some(try_frame) = self.try_stack.pop() {
                        // tryフレームが存在するチャンクまでフレームを巻き戻す
                        while let Some(frame) = self.frames.last() {
                            if frame.chunk_id == try_frame.chunk_id {
                                break;
                            }
                            let finished_frame = self.frames.pop().ok_or_else(|| RuntimeError {
                                message: "フレームスタックが空です".into(),
                            })?;
                            let callee_pos = if finished_frame.base > 0 {
                                finished_frame.base - 1
                            } else {
                                0
                            };
                            self.stack.truncate(callee_pos);
                        }

                        if let Some(frame) = self.frames.last_mut() {
                            if frame.chunk_id == try_frame.chunk_id {
                                // スタックをtry開始時の深さに戻し、例外値をcatchに渡す
                                self.stack.truncate(try_frame.stack_depth);
                                self.stack.push(exception.clone());
                                frame.ip = try_frame.catch_target;
                                handled = true;
                                break;
                            }
                        }
                    }

                    if !handled {
                        return Err(RuntimeError {
                            message: format!("捕捉されない例外: {}", exception.to_display_string()),
                        });
                    }
                }

                OpCode::_Suspend | OpCode::_Resume => {
                    return Err(RuntimeError {
                        message: "非同期機能は未実装です".into(),
                    });
                }
            }
        }
    }

    fn pop_value(&mut self) -> Result<Value, RuntimeError> {
        self.stack.pop().ok_or_else(|| RuntimeError {
            message: "スタックが空です".into(),
        })
    }

    // === 算術演算 ===

    fn arithmetic_add(&self, a: &Value, b: &Value) -> Result<Value, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x + y)),
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
            (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(x.to_f64().unwrap_or(0.0) + y)),
            (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x + y.to_f64().unwrap_or(0.0))),
            (Value::String(x), Value::String(y)) => Ok(Value::String(format!("{}{}", x, y))),
            _ => Err(RuntimeError {
                message: format!("{}と{}の和は計算できません", a.type_name(), b.type_name()),
            }),
        }
    }

    fn arithmetic_sub(&self, a: &Value, b: &Value) -> Result<Value, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x - y)),
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
            (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(x.to_f64().unwrap_or(0.0) - y)),
            (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x - y.to_f64().unwrap_or(0.0))),
            _ => Err(RuntimeError {
                message: format!("{}と{}の差は計算できません", a.type_name(), b.type_name()),
            }),
        }
    }

    fn arithmetic_mul(&self, a: &Value, b: &Value) -> Result<Value, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x * y)),
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
            (Value::Integer(x), Value::Float(y)) => Ok(Value::Float(x.to_f64().unwrap_or(0.0) * y)),
            (Value::Float(x), Value::Integer(y)) => Ok(Value::Float(x * y.to_f64().unwrap_or(0.0))),
            _ => Err(RuntimeError {
                message: format!("{}と{}の積は計算できません", a.type_name(), b.type_name()),
            }),
        }
    }

    fn arithmetic_div(&self, a: &Value, b: &Value) -> Result<Value, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => {
                if y == &BigInt::from(0) {
                    return Err(RuntimeError {
                        message: "ゼロで割ることはできません".into(),
                    });
                }
                // 整数÷整数 → 常にFloat（割り切れる場合も）
                Ok(Value::Float(
                    x.to_f64().unwrap_or(0.0) / y.to_f64().unwrap_or(1.0),
                ))
            }
            (Value::Float(x), Value::Float(y)) => {
                if *y == 0.0 {
                    // 小数÷0 → NaN
                    return Ok(Value::Float(f64::NAN));
                }
                Ok(Value::Float(x / y))
            }
            (Value::Integer(x), Value::Float(y)) => {
                if *y == 0.0 {
                    // 小数÷0 → NaN
                    return Ok(Value::Float(f64::NAN));
                }
                Ok(Value::Float(x.to_f64().unwrap_or(0.0) / y))
            }
            (Value::Float(x), Value::Integer(y)) => {
                if y == &BigInt::from(0) {
                    // 小数÷0 → NaN
                    return Ok(Value::Float(f64::NAN));
                }
                Ok(Value::Float(x / y.to_f64().unwrap_or(1.0)))
            }
            _ => Err(RuntimeError {
                message: format!("{}を{}で割ることはできません", a.type_name(), b.type_name()),
            }),
        }
    }

    fn arithmetic_mod(&self, a: &Value, b: &Value) -> Result<Value, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => {
                if y == &BigInt::from(0) {
                    return Err(RuntimeError {
                        message: "ゼロで割ることはできません".into(),
                    });
                }
                Ok(Value::Integer(x % y))
            }
            _ => Err(RuntimeError {
                message: format!("{}と{}の余りは計算できません", a.type_name(), b.type_name()),
            }),
        }
    }

    // === 比較 ===

    fn compare_gt(&self, a: &Value, b: &Value) -> Result<bool, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => Ok(x > y),
            (Value::Float(x), Value::Float(y)) => Ok(x > y),
            (Value::Integer(x), Value::Float(y)) => Ok(x.to_f64().unwrap_or(0.0) > *y),
            (Value::Float(x), Value::Integer(y)) => Ok(*x > y.to_f64().unwrap_or(0.0)),
            (Value::String(x), Value::String(y)) => Ok(x > y),
            _ => Err(RuntimeError {
                message: format!("{}と{}は比較できません", a.type_name(), b.type_name()),
            }),
        }
    }

    fn compare_lt(&self, a: &Value, b: &Value) -> Result<bool, RuntimeError> {
        match (a, b) {
            (Value::Integer(x), Value::Integer(y)) => Ok(x < y),
            (Value::Float(x), Value::Float(y)) => Ok(x < y),
            (Value::Integer(x), Value::Float(y)) => Ok(x.to_f64().unwrap_or(0.0) < *y),
            (Value::Float(x), Value::Integer(y)) => Ok(*x < y.to_f64().unwrap_or(0.0)),
            (Value::String(x), Value::String(y)) => Ok(x < y),
            _ => Err(RuntimeError {
                message: format!("{}と{}は比較できません", a.type_name(), b.type_name()),
            }),
        }
    }
}
