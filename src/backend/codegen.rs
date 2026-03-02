use num_bigint::BigInt;

use crate::ast::*;
use crate::bytecode::{Chunk, OpCode, ProcRef, Value};
use crate::sema::hir::TypedHirProgram;

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

#[derive(Debug, Default)]
struct LoopContext {
    continue_target: usize,
    continue_jumps: Vec<usize>,
    break_jumps: Vec<usize>,
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
    /// ループのジャンプパッチ情報
    loop_stack: Vec<LoopContext>,
    /// 一時変数名採番
    temp_counter: usize,
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
            loop_stack: Vec::new(),
            temp_counter: 0,
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

    /// TypedHIR をコンパイルする（現段階では AST ベース実装へ委譲）
    pub fn compile_typed(self, typed: &TypedHirProgram) -> Result<Vec<Chunk>, Vec<CompileError>> {
        self.compile(&typed.program)
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
                let jump = self.emit(OpCode::Jump(0));
                if let Some(ctx) = self.loop_stack.last_mut() {
                    match &stmt.kind {
                        StmtKind::Continue => ctx.continue_jumps.push(jump),
                        StmtKind::Break => ctx.break_jumps.push(jump),
                        _ => {}
                    }
                } else {
                    self.errors.push(CompileError {
                        message: match &stmt.kind {
                            StmtKind::Continue => "「次へ」はループの中でのみ使えます".into(),
                            StmtKind::Break => "「抜ける」はループの中でのみ使えます".into(),
                            _ => unreachable!(),
                        },
                    });
                }
            }

            StmtKind::Use { .. } => {
                // モジュール解決フェーズで消費済み。コード生成時は no-op。
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

        // 本体のコンパイル（最後の式の値を残す）
        self.compile_block(body, true);

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
                    KosoAdoKind::Kou => {
                        self.emit(OpCode::LoadCurrentProc);
                        return;
                    }
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

            ExprKind::Loop(kind) => {
                self.compile_loop(kind);
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

            ExprKind::TryCatch {
                body,
                catch_param,
                catch_body,
                finally_body,
            } => {
                // SetupTry: catchラベルは後でパッチ
                let setup_pos = self.emit(OpCode::SetupTry(0));

                // try ボディ
                self.compile_block(body, true);
                self.emit(OpCode::EndTry);
                let jump_finally = self.emit(OpCode::Jump(0));

                // catch ラベル
                let catch_pos = self.chunks[self.current_chunk_idx].current_pos();
                self.chunks[self.current_chunk_idx].patch_jump(setup_pos, catch_pos);

                self.begin_scope();
                // catchパラメータがあればローカル変数に保存
                if let Some(param) = catch_param.as_ref() {
                    let idx = self.add_local(param.clone());
                    self.emit(OpCode::StoreLocal(idx)); // 例外値を捕捉
                    self.emit(OpCode::Pop); // StoreLocalが参照するスタック値を明示的に消費
                } else {
                    // catchパラメータを使わない場合は例外値を破棄
                    self.emit(OpCode::Pop);
                }

                if let Some(catch) = catch_body {
                    self.compile_block(catch, true);
                } else {
                    self.emit(OpCode::PushNone);
                }
                self.end_scope();

                // finally ラベル
                let finally_pos = self.chunks[self.current_chunk_idx].current_pos();
                self.chunks[self.current_chunk_idx].patch_jump(jump_finally, finally_pos);

                // finally（必ず行う）ブロックがあれば実行し、値は破棄
                if let Some(finally) = finally_body {
                    self.compile_block(finally, false);
                }
            }

            ExprKind::Throw(expr) => {
                self.compile_expr(expr);
                self.emit(OpCode::Throw);
            }

            ExprKind::Match { .. } => {
                self.errors.push(CompileError {
                    message: "「場合分け」は現在未実装です".into(),
                });
                self.emit(OpCode::PushNone);
            }
            ExprKind::TeChain { .. } => {
                self.errors.push(CompileError {
                    message: "て形チェイン実行は現在未実装です".into(),
                });
                self.emit(OpCode::PushNone);
            }
            ExprKind::BranchChain { .. } => {
                self.errors.push(CompileError {
                    message: "「分岐して」実行は現在未実装です".into(),
                });
                self.emit(OpCode::PushNone);
            }
            ExprKind::MethodCall { .. } => {
                self.errors.push(CompileError {
                    message: "メソッド呼び出し実行は現在未実装です".into(),
                });
                self.emit(OpCode::PushNone);
            }
            ExprKind::Construct { .. } => {
                self.errors.push(CompileError {
                    message: "組インスタンス生成は現在未実装です".into(),
                });
                self.emit(OpCode::PushNone);
            }
            ExprKind::Destructure { .. } => {
                self.errors.push(CompileError {
                    message: "分解束縛実行は現在未実装です".into(),
                });
                self.emit(OpCode::PushNone);
            }
        }
    }

    fn compile_call(&mut self, callee: &str, args: &[ParticleArg]) {
        // 特殊な組み込み手順
        match callee {
            "表示する" => {
                // 引数をコンパイル (「と」引数を探す)
                if let Some(arg) = args
                    .iter()
                    .find(|a| a.particle == crate::token::Particle::To)
                {
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
        if callee == "こう" {
            self.emit(OpCode::LoadCurrentProc);
        } else if let Some(idx) = self.resolve_local(callee) {
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

        self.compile_block(then_block, true);

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

            self.compile_block(elif_block, true);
            end_jumps.push(self.emit(OpCode::Jump(0)));
        }

        let else_pos = self.chunks[self.current_chunk_idx].current_pos();
        self.chunks[self.current_chunk_idx].patch_jump(current_false, else_pos);
        self.emit(OpCode::Pop);

        if let Some(else_blk) = else_block {
            self.compile_block(else_blk, true);
        } else {
            self.emit(OpCode::PushNone);
        }

        let end_pos = self.chunks[self.current_chunk_idx].current_pos();
        for jump in end_jumps {
            self.chunks[self.current_chunk_idx].patch_jump(jump, end_pos);
        }
    }

    fn compile_block(&mut self, block: &Block, keep_last_value: bool) {
        self.begin_scope();
        let stmt_count = block.statements.len();
        for (i, stmt) in block.statements.iter().enumerate() {
            self.compile_stmt(stmt);

            // expression-based: ブロックの最後の式の値を残す（値モードのみ）
            if keep_last_value && i == stmt_count - 1 {
                if let StmtKind::ExprStmt(_) = &stmt.kind {
                    // 最後の stmt で Pop が追加されているので取り消す
                    if let Some(OpCode::Pop) = self.chunks[self.current_chunk_idx].code.last() {
                        self.chunks[self.current_chunk_idx].code.pop();
                    }
                }
            }
        }

        if keep_last_value && stmt_count == 0 {
            self.emit(OpCode::PushNone);
        }

        self.end_scope();
    }

    fn compile_loop(&mut self, kind: &LoopKind) {
        match kind {
            LoopKind::Times { count, var, body } => self.compile_times_loop(count, var, body),
            LoopKind::Range {
                from,
                to,
                var,
                body,
            } => self.compile_range_loop(from, to, var, body),
            LoopKind::While { condition, body } => self.compile_while_loop(condition, body),
            LoopKind::ForEach { .. } => {
                self.errors.push(CompileError {
                    message: "「それぞれについて」は現在未実装です".into(),
                });
            }
        }
        // ループ式の値は常に 無
        self.emit(OpCode::PushNone);
    }

    fn compile_times_loop(&mut self, count: &Expr, var: &Option<String>, body: &Block) {
        let count_name = self.fresh_temp("__回数");
        let index_name = self.fresh_temp("__回数i");

        self.compile_expr(count);
        self.emit(OpCode::StoreGlobal(count_name.clone()));
        self.emit(OpCode::Pop);

        let one_const = self.add_constant(Value::Integer(BigInt::from(1)));
        self.emit(OpCode::Constant(one_const));
        self.emit(OpCode::StoreGlobal(index_name.clone()));
        self.emit(OpCode::Pop);

        let loop_start = self.current_chunk().current_pos();
        self.emit(OpCode::LoadGlobal(index_name.clone()));
        self.emit(OpCode::LoadGlobal(count_name.clone()));
        self.emit(OpCode::Greater);
        let jump_body = self.emit(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);
        let jump_exit = self.emit(OpCode::Jump(0));

        let body_start = self.current_chunk().current_pos();
        self.current_chunk().patch_jump(jump_body, body_start);
        self.emit(OpCode::Pop);

        self.loop_stack.push(LoopContext::default());
        if let Some(loop_var) = var {
            self.emit(OpCode::LoadGlobal(index_name.clone()));
            if let Some(idx) = self.resolve_local(loop_var) {
                self.emit(OpCode::StoreLocal(idx));
            } else {
                self.emit(OpCode::StoreGlobal(loop_var.clone()));
            }
            self.emit(OpCode::Pop);
        }
        self.compile_block(body, false);

        let continue_target = self.current_chunk().current_pos();
        if let Some(ctx) = self.loop_stack.last_mut() {
            ctx.continue_target = continue_target;
        }

        self.emit(OpCode::LoadGlobal(index_name.clone()));
        let one_const = self.add_constant(Value::Integer(BigInt::from(1)));
        self.emit(OpCode::Constant(one_const));
        self.emit(OpCode::Add);
        self.emit(OpCode::StoreGlobal(index_name));
        self.emit(OpCode::Pop);
        self.emit(OpCode::Jump(loop_start));

        let exit_pos = self.current_chunk().current_pos();
        self.current_chunk().patch_jump(jump_exit, exit_pos);
        self.patch_loop_control(exit_pos);
    }

    fn compile_range_loop(&mut self, from: &Expr, to: &Expr, var: &Option<String>, body: &Block) {
        let end_name = self.fresh_temp("__範囲終");
        let index_name = self.fresh_temp("__範囲i");

        self.compile_expr(to);
        self.emit(OpCode::StoreGlobal(end_name.clone()));
        self.emit(OpCode::Pop);

        self.compile_expr(from);
        self.emit(OpCode::StoreGlobal(index_name.clone()));
        self.emit(OpCode::Pop);

        let loop_start = self.current_chunk().current_pos();
        self.emit(OpCode::LoadGlobal(index_name.clone()));
        self.emit(OpCode::LoadGlobal(end_name.clone()));
        self.emit(OpCode::Greater);
        let jump_body = self.emit(OpCode::JumpIfFalse(0));
        self.emit(OpCode::Pop);
        let jump_exit = self.emit(OpCode::Jump(0));

        let body_start = self.current_chunk().current_pos();
        self.current_chunk().patch_jump(jump_body, body_start);
        self.emit(OpCode::Pop);

        self.loop_stack.push(LoopContext::default());
        if let Some(loop_var) = var {
            self.emit(OpCode::LoadGlobal(index_name.clone()));
            if let Some(idx) = self.resolve_local(loop_var) {
                self.emit(OpCode::StoreLocal(idx));
            } else {
                self.emit(OpCode::StoreGlobal(loop_var.clone()));
            }
            self.emit(OpCode::Pop);
        }
        self.compile_block(body, false);

        let continue_target = self.current_chunk().current_pos();
        if let Some(ctx) = self.loop_stack.last_mut() {
            ctx.continue_target = continue_target;
        }

        self.emit(OpCode::LoadGlobal(index_name.clone()));
        let one_const = self.add_constant(Value::Integer(BigInt::from(1)));
        self.emit(OpCode::Constant(one_const));
        self.emit(OpCode::Add);
        self.emit(OpCode::StoreGlobal(index_name));
        self.emit(OpCode::Pop);
        self.emit(OpCode::Jump(loop_start));

        let exit_pos = self.current_chunk().current_pos();
        self.current_chunk().patch_jump(jump_exit, exit_pos);
        self.patch_loop_control(exit_pos);
    }

    fn compile_while_loop(&mut self, condition: &Expr, body: &Block) {
        let loop_start = self.current_chunk().current_pos();
        self.compile_expr(condition);

        let jump_body = self.emit(OpCode::JumpIfTrue(0));
        self.emit(OpCode::Pop);
        let jump_exit = self.emit(OpCode::Jump(0));

        let body_start = self.current_chunk().current_pos();
        self.current_chunk().patch_jump(jump_body, body_start);
        self.emit(OpCode::Pop);

        self.loop_stack.push(LoopContext {
            continue_target: loop_start,
            continue_jumps: Vec::new(),
            break_jumps: Vec::new(),
        });
        self.compile_block(body, false);
        self.emit(OpCode::Jump(loop_start));

        let exit_pos = self.current_chunk().current_pos();
        self.current_chunk().patch_jump(jump_exit, exit_pos);
        self.patch_loop_control(exit_pos);
    }

    fn patch_loop_control(&mut self, exit_pos: usize) {
        if let Some(ctx) = self.loop_stack.pop() {
            for jump in ctx.continue_jumps {
                self.current_chunk().patch_jump(jump, ctx.continue_target);
            }
            for jump in ctx.break_jumps {
                self.current_chunk().patch_jump(jump, exit_pos);
            }
        }
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

    fn fresh_temp(&mut self, prefix: &str) -> String {
        let id = self.temp_counter;
        self.temp_counter += 1;
        format!("{prefix}_{id}")
    }
}
