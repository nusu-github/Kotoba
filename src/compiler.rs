use num_bigint::BigInt;

use crate::ast::*;
use crate::bytecode::{Chunk, OpCode, ProcRef, Value};

/// コンパイルエラー
#[derive(Debug)]
pub struct CompileError {
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "コンパイルエラー: {}", self.message)
    }
}

/// ローカル変数情報
#[derive(Debug, Clone)]
struct Local {
    name: String,
    depth: usize,
}

/// コンパイラ: AST → バイトコード
pub struct Compiler {
    /// 生成されたチャンク一覧
    chunks: Vec<Chunk>,
    /// 現在コンパイル中のチャンクのインデックス
    current_chunk_idx: usize,
    /// ローカル変数スタック
    locals: Vec<Local>,
    /// スコープの深さ
    scope_depth: usize,
    /// エラー一覧
    errors: Vec<CompileError>,
}

impl Compiler {
    pub fn new() -> Self {
        let main_chunk = Chunk::new("<メイン>", 0);
        Self {
            chunks: vec![main_chunk],
            current_chunk_idx: 0,
            locals: Vec::new(),
            scope_depth: 0,
            errors: Vec::new(),
        }
    }

    /// プログラムをコンパイルする
    pub fn compile(mut self, program: &Program) -> Result<Vec<Chunk>, Vec<CompileError>> {
        for stmt in &program.statements {
            self.compile_stmt(stmt);
        }

        self.current_chunk().emit(OpCode::Halt);

        if self.errors.is_empty() {
            Ok(self.chunks)
        } else {
            Err(self.errors)
        }
    }

    fn current_chunk(&mut self) -> &mut Chunk {
        &mut self.chunks[self.current_chunk_idx]
    }

    fn emit(&mut self, op: OpCode) -> usize {
        self.chunks[self.current_chunk_idx].emit(op)
    }

    fn add_constant(&mut self, value: Value) -> usize {
        self.chunks[self.current_chunk_idx].add_constant(value)
    }

    // === 文のコンパイル ===

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Bind {
                name,
                mutable: _,
                value,
            } => {
                self.compile_expr(value);
                if self.scope_depth == 0 {
                    self.emit(OpCode::StoreGlobal(name.clone()));
                } else {
                    let idx = self.add_local(name.clone());
                    self.emit(OpCode::StoreLocal(idx));
                }
            }

            StmtKind::Rebind { name, value } => {
                self.compile_expr(value);
                if let Some(idx) = self.resolve_local(name) {
                    self.emit(OpCode::StoreLocal(idx));
                } else {
                    self.emit(OpCode::StoreGlobal(name.clone()));
                }
            }

            StmtKind::ExprStmt(expr) => {
                self.compile_expr(expr);
                // 式文の結果を「それ」(__それ) に自動保存
                self.emit(OpCode::StoreGlobal("__それ".to_string()));
                self.emit(OpCode::Pop);
            }

            StmtKind::Return(value) => {
                if let Some(val) = value {
                    self.compile_expr(val);
                } else {
                    self.emit(OpCode::PushNone);
                }
                self.emit(OpCode::Return);
            }

            StmtKind::ProcDef {
                name, params, body, ..
            } => {
                self.compile_proc_def(name, params, body);
            }

            StmtKind::Continue | StmtKind::Break => {
                // TODO: ループ制御のジャンプ
                self.errors.push(CompileError {
                    message: "「次へ」「抜ける」は現在未実装です".into(),
                });
            }

            StmtKind::Use { module, .. } => {
                // TODO: モジュールシステム
                self.errors.push(CompileError {
                    message: format!("モジュール「{}」のインポートは現在未実装です", module),
                });
            }

            StmtKind::StructDef { name, .. } => {
                // TODO: 組定義
                self.errors.push(CompileError {
                    message: format!("組「{}」の定義は現在未実装です", name),
                });
            }

            StmtKind::TraitDef { name, .. } => {
                self.errors.push(CompileError {
                    message: format!("特性「{}」の定義は現在未実装です", name),
                });
            }

            StmtKind::TraitImpl { .. } => {
                self.errors.push(CompileError {
                    message: "特性の実装は現在未実装です".into(),
                });
            }
        }
    }

    fn compile_proc_def(&mut self, name: &str, params: &[Param], body: &Block) {
        // 新しいチャンクを作成
        let new_chunk = Chunk::new(name, params.len());
        let chunk_id = self.chunks.len();
        self.chunks.push(new_chunk);

        // 現在のコンパイル状態を保存
        let prev_chunk_idx = self.current_chunk_idx;
        let prev_locals = std::mem::take(&mut self.locals);
        let prev_depth = self.scope_depth;

        self.current_chunk_idx = chunk_id;
        self.scope_depth = 1;
        self.locals = Vec::new();

        // パラメータをローカル変数として登録
        for param in params {
            let param_name = param
                .name
                .clone()
                .unwrap_or_else(|| param.particle.as_str().to_string());
            self.add_local(param_name);
        }

        // 本体のコンパイル
        for stmt in &body.statements {
            self.compile_stmt(stmt);
        }

        // 暗黙の return (最後の値を返す)
        // 最後の命令が Return でなければ、PushNone + Return を追加
        let last_is_return = self.chunks[self.current_chunk_idx]
            .code
            .last()
            .is_some_and(|op| matches!(op, OpCode::Return));

        if !last_is_return {
            // 最後の Pop を取り除いて Return に置き換える
            if let Some(OpCode::Pop) = self.chunks[self.current_chunk_idx].code.last() {
                self.chunks[self.current_chunk_idx].code.pop();
            } else {
                self.emit(OpCode::PushNone);
            }
            self.emit(OpCode::Return);
        }

        self.chunks[self.current_chunk_idx].local_count = self.locals.len();

        // 状態を復元
        self.current_chunk_idx = prev_chunk_idx;
        self.locals = prev_locals;
        self.scope_depth = prev_depth;

        // 手順を値としてグローバルに登録
        let proc_ref = Value::Procedure(ProcRef {
            name: name.to_string(),
            chunk_id,
            arity: params.len(),
        });
        let const_idx = self.add_constant(proc_ref);
        self.emit(OpCode::Constant(const_idx));
        if self.scope_depth == 0 {
            self.emit(OpCode::StoreGlobal(name.to_string()));
        } else {
            let idx = self.add_local(name.to_string());
            self.emit(OpCode::StoreLocal(idx));
        }
    }

    // === 式のコンパイル ===

    fn compile_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Integer(n) => {
                let big = n.parse::<BigInt>().unwrap_or_default();
                let const_idx = self.add_constant(Value::Integer(big));
                self.emit(OpCode::Constant(const_idx));
            }

            ExprKind::Float(n) => {
                let f = n.parse::<f64>().unwrap_or(0.0);
                let const_idx = self.add_constant(Value::Float(f));
                self.emit(OpCode::Constant(const_idx));
            }

            ExprKind::StringLiteral(s) => {
                let const_idx = self.add_constant(Value::String(s.clone()));
                self.emit(OpCode::Constant(const_idx));
            }

            ExprKind::StringInterp(parts) => {
                let count = parts.len();
                for part in parts {
                    match part {
                        StringPart::Literal(s) => {
                            let const_idx = self.add_constant(Value::String(s.clone()));
                            self.emit(OpCode::Constant(const_idx));
                        }
                        StringPart::Expr(e) => {
                            self.compile_expr(e);
                        }
                    }
                }
                self.emit(OpCode::Concat(count));
            }

            ExprKind::Bool(b) => {
                if *b {
                    self.emit(OpCode::PushTrue);
                } else {
                    self.emit(OpCode::PushFalse);
                }
            }

            ExprKind::None => {
                self.emit(OpCode::PushNone);
            }

            ExprKind::Identifier(name) => {
                if let Some(idx) = self.resolve_local(name) {
                    self.emit(OpCode::LoadLocal(idx));
                } else {
                    self.emit(OpCode::LoadGlobal(name.clone()));
                }
            }

            ExprKind::KosoAdo(kind) => {
                // こそあどは現時点では単純な変数参照に変換
                let name = match kind {
                    KosoAdoKind::Kore => "__これ",
                    KosoAdoKind::Sore => "__それ",
                    KosoAdoKind::Are => "__あれ",
                    KosoAdoKind::Kou => "__こう",
                    KosoAdoKind::Koko => "__ここ",
                    KosoAdoKind::Soko => "__そこ",
                };
                self.emit(OpCode::LoadGlobal(name.to_string()));
            }

            ExprKind::List(elements) => {
                let count = elements.len();
                for elem in elements {
                    self.compile_expr(elem);
                }
                self.emit(OpCode::BuildList(count));
            }

            ExprKind::Map(entries) => {
                let count = entries.len();
                for (key, value) in entries {
                    let key_const = self.add_constant(Value::String(key.clone()));
                    self.emit(OpCode::Constant(key_const));
                    self.compile_expr(value);
                }
                self.emit(OpCode::BuildMap(count));
            }

            ExprKind::Call { callee, args } => {
                self.compile_call(callee, args);
            }

            ExprKind::PropertyAccess { object, property } => {
                self.compile_expr(object);
                self.emit(OpCode::GetProperty(property.clone()));
            }

            ExprKind::BinaryOp { op, left, right } => {
                self.compile_expr(left);
                self.compile_expr(right);
                let opcode = match op {
                    BinOp::Add => OpCode::Add,
                    BinOp::Sub => OpCode::Sub,
                    BinOp::Mul => OpCode::Mul,
                    BinOp::Div => OpCode::Div,
                    BinOp::Mod => OpCode::Mod,
                };
                self.emit(opcode);
            }

            ExprKind::UnaryOp { op, operand } => {
                self.compile_expr(operand);
                match op {
                    UnaryOp::Not => {
                        self.emit(OpCode::Not);
                    }
                }
            }

            ExprKind::Comparison { op, left, right } => {
                self.compile_expr(left);
                self.compile_expr(right);
                let opcode = match op {
                    CompOp::Gt => OpCode::Greater,
                    CompOp::Lt => OpCode::Less,
                    CompOp::Eq => OpCode::Equal,
                    CompOp::Ne => OpCode::NotEqual,
                    CompOp::Ge => OpCode::GreaterEqual,
                    CompOp::Le => OpCode::LessEqual,
                };
                self.emit(opcode);
            }

            ExprKind::Logical { op, left, right } => {
                self.compile_expr(left);
                match op {
                    LogicalOp::And => {
                        let jump_pos = self.emit(OpCode::JumpIfFalse(0));
                        self.emit(OpCode::Pop);
                        self.compile_expr(right);
                        let end = self.chunks[self.current_chunk_idx].current_pos();
                        self.chunks[self.current_chunk_idx].patch_jump(jump_pos, end);
                    }
                    LogicalOp::Or => {
                        let jump_pos = self.emit(OpCode::JumpIfTrue(0));
                        self.emit(OpCode::Pop);
                        self.compile_expr(right);
                        let end = self.chunks[self.current_chunk_idx].current_pos();
                        self.chunks[self.current_chunk_idx].patch_jump(jump_pos, end);
                    }
                }
            }

            ExprKind::If {
                condition,
                then_block,
                elif_clauses,
                else_block,
            } => {
                self.compile_if(condition, then_block, elif_clauses, else_block);
            }

            ExprKind::Lambda { params, body } => {
                let name = "<無名手順>";
                self.compile_proc_def(name, params, body);
            }

            ExprKind::WithCounter { value, counter } => {
                // 助数詞付き数値はとりあえず値だけコンパイル（型チェックは将来）
                self.compile_expr(value);
                let _ = counter;
            }

            ExprKind::Throw(expr) => {
                self.compile_expr(expr);
                // TODO: 例外機構
                self.emit(OpCode::Print);
                self.emit(OpCode::Halt);
            }

            ExprKind::TryCatch { body, catch_param, catch_body, finally_body } => {
                // SetupTry: catchラベルは後でパッチ
                let setup_pos = self.emit(OpCode::SetupTry(0));

                // try ボディ
                self.compile_block(body);
                self.emit(OpCode::EndTry);
                let jump_end = self.emit(OpCode::Jump(0));

                // catch ラベル
                let catch_pos = self.chunks[self.current_chunk_idx].current_pos();
                self.chunks[self.current_chunk_idx].patch_jump(setup_pos, catch_pos);

                // catchパラメータがあればローカル変数に保存、なければ例外値を破棄
                if let Some(param) = catch_param {
                    let idx = self.add_local(param.clone());
                    self.emit(OpCode::StoreLocal(idx));
                    self.emit(OpCode::Pop);
                } else {
                    self.emit(OpCode::Pop);
                }

                if let Some(catch) = catch_body {
                    self.compile_block(catch);
                } else {
                    self.emit(OpCode::PushNone);
                }

                // end ラベル
                let end_pos = self.chunks[self.current_chunk_idx].current_pos();
                self.chunks[self.current_chunk_idx].patch_jump(jump_end, end_pos);

                // finally（必ず行う）ブロックがあれば実行し、値は破棄
                if let Some(finally) = finally_body {
                    self.compile_block(finally);
                    self.emit(OpCode::Pop);
                }
            }

            ExprKind::Throw(expr) => {
                self.compile_expr(expr);
                self.emit(OpCode::Throw);
            }

            ExprKind::Match { .. }
            | ExprKind::Loop(_)
            | ExprKind::TeChain { .. }
            | ExprKind::BranchChain { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::Construct { .. }
            | ExprKind::Destructure { .. } => {
                // TODO: 残りの式の種類
                self.emit(OpCode::PushNone);
            }
        }
    }

    fn compile_call(&mut self, callee: &str, args: &[ParticleArg]) {
        // 特殊な組み込み手順
        match callee {
            "表示する" => {
                // 引数をコンパイル (「と」引数を探す)
                if let Some(arg) = args.iter().find(|a| a.particle == crate::token::Particle::To) {
                    self.compile_expr(&arg.value);
                } else if let Some(arg) = args.first() {
                    self.compile_expr(&arg.value);
                } else {
                    self.emit(OpCode::PushNone);
                }
                self.emit(OpCode::Print);
                self.emit(OpCode::PushNone); // 表示するは無を返す
                return;
            }
            "変える" => {
                // 再束縛: `名前を 式に変える`
                if args.len() >= 2 {
                    let target = &args[0]; // `を` 引数 = 変数名
                    let new_value = &args[1]; // `に` 引数 = 新しい値
                    self.compile_expr(&new_value.value);
                    if let ExprKind::Identifier(name) = &target.value.kind {
                        if let Some(idx) = self.resolve_local(name) {
                            self.emit(OpCode::StoreLocal(idx));
                        } else {
                            self.emit(OpCode::StoreGlobal(name.clone()));
                        }
                    }
                    self.emit(OpCode::PushNone);
                    return;
                }
            }
            _ => {}
        }

        // 一般的な手順呼び出し
        // 呼び出し先をロード
        if let Some(idx) = self.resolve_local(callee) {
            self.emit(OpCode::LoadLocal(idx));
        } else {
            self.emit(OpCode::LoadGlobal(callee.to_string()));
        }

        // 引数をコンパイル
        let arity = args.len();
        for arg in args {
            self.compile_expr(&arg.value);
        }

        self.emit(OpCode::Call(arity));
    }

    fn compile_if(
        &mut self,
        condition: &Expr,
        then_block: &Block,
        elif_clauses: &[(Expr, Block)],
        else_block: &Option<Block>,
    ) {
        self.compile_expr(condition);
        let then_jump = self.emit(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);

        self.compile_block(then_block);

        let mut end_jumps = vec![];
        end_jumps.push(self.emit(OpCode::Jump(0)));

        let mut current_false = then_jump;

        for (elif_cond, elif_block) in elif_clauses {
            let pos = self.chunks[self.current_chunk_idx].current_pos();
            self.chunks[self.current_chunk_idx].patch_jump(current_false, pos);
            self.emit(OpCode::Pop);

            self.compile_expr(elif_cond);
            current_false = self.emit(OpCode::JumpIfFalse(0));
            self.emit(OpCode::Pop);

            self.compile_block(elif_block);
            end_jumps.push(self.emit(OpCode::Jump(0)));
        }

        let else_pos = self.chunks[self.current_chunk_idx].current_pos();
        self.chunks[self.current_chunk_idx].patch_jump(current_false, else_pos);
        self.emit(OpCode::Pop);

        if let Some(else_blk) = else_block {
            self.compile_block(else_blk);
        } else {
            self.emit(OpCode::PushNone);
        }

        let end_pos = self.chunks[self.current_chunk_idx].current_pos();
        for jump in end_jumps {
            self.chunks[self.current_chunk_idx].patch_jump(jump, end_pos);
        }
    }

    fn compile_block(&mut self, block: &Block) {
        self.begin_scope();
        let stmt_count = block.statements.len();
        for (i, stmt) in block.statements.iter().enumerate() {
            self.compile_stmt(stmt);

            // expression-based: ブロックの最後の式の値を残す
            // ただし ExprStmt の Pop を最後の式だけ取り消す
            if i == stmt_count - 1 {
                if let StmtKind::ExprStmt(_) = &stmt.kind {
                    // 最後の stmt で Pop が追加されているので取り消す
                    if let Some(OpCode::Pop) = self.chunks[self.current_chunk_idx].code.last() {
                        self.chunks[self.current_chunk_idx].code.pop();
                    }
                }
            }
        }

        if stmt_count == 0 {
            self.emit(OpCode::PushNone);
        }

        self.end_scope();
    }

    // === スコープ管理 ===

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        self.scope_depth -= 1;
        // スコープを抜ける際にローカル変数を片付ける
        while let Some(local) = self.locals.last() {
            if local.depth > self.scope_depth {
                self.locals.pop();
                // スタックからはPopしない（値はブロックの値として使われる可能性があるため）
            } else {
                break;
            }
        }
    }

    fn add_local(&mut self, name: String) -> usize {
        let idx = self.locals.len();
        self.locals.push(Local {
            name,
            depth: self.scope_depth,
        });
        self.chunks[self.current_chunk_idx].local_count = self.locals.len();
        idx
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        for (i, local) in self.locals.iter().enumerate().rev() {
            if local.name == name {
                return Some(i);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn compile(input: &str) -> Vec<Chunk> {
        let (tokens, lex_errors) = Lexer::new(input).tokenize();
        assert!(lex_errors.is_empty(), "レキサーエラー: {:?}", lex_errors);
        let (program, parse_errors) = Parser::new(tokens).parse();
        assert!(
            parse_errors.is_empty(),
            "パーサエラー: {:?}",
            parse_errors
        );
        let compiler = Compiler::new();
        compiler.compile(&program).expect("コンパイルエラー")
    }

    #[test]
    fn test_compile_binding() {
        let chunks = compile("名前 は 「太郎」");
        // メインチャンクに StoreGlobal が含まれるはず
        let main = &chunks[0];
        assert!(main
            .code
            .iter()
            .any(|op| matches!(op, OpCode::StoreGlobal(n) if n == "名前")));
    }

    #[test]
    fn test_compile_print() {
        let chunks = compile("「こんにちは」と 表示する");
        let main = &chunks[0];
        assert!(main.code.iter().any(|op| matches!(op, OpCode::Print)));
    }

    #[test]
    fn test_compile_proc_def() {
        let chunks = compile("二乗する という 手順【x:を】\n  xとxの積を 返す");
        // メインチャンク + 手順チャンク = 2つ
        assert!(chunks.len() >= 2);
        assert_eq!(chunks[1].name, "二乗する");
        assert_eq!(chunks[1].arity, 1);
    }

    #[test]
    fn test_compile_if() {
        let chunks = compile("もし 真 ならば\n  「はい」と 表示する\nそうでなければ\n  「いいえ」と 表示する");
        let main = &chunks[0];
        assert!(main
            .code
            .iter()
            .any(|op| matches!(op, OpCode::JumpIfFalse(_))));
    }
}
