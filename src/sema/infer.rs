use std::collections::{HashMap, HashSet};

use crate::diag::report::{Diagnostic, DiagnosticKind};
use crate::frontend::ast::{
    Block, ChainStep, Expr, ExprKind, KosoAdoKind, LoopKind, Param, ParticleArg, Program, Stmt,
    StmtKind, StringPart,
};
use crate::frontend::token::Particle;
use crate::sema::hir::{TypedHirProgram, lower_to_typed_hir};

const DEFAULT_ANALYZE_STEP_LIMIT: usize = 500_000;

#[derive(Debug, Clone)]
struct ProcSignature {
    particles: Vec<Particle>,
    particle_count: usize,
}

#[derive(Debug, Clone)]
struct StructInfo {
    fields: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct TraitMethodSig {
    particles: Vec<Particle>,
    return_type: Option<String>,
}

#[derive(Debug, Clone)]
struct TraitInfo {
    methods: HashMap<String, TraitMethodSig>,
}

#[derive(Debug)]
struct Analyzer {
    proc_signatures: HashMap<String, Vec<ProcSignature>>,
    struct_defs: HashMap<String, StructInfo>,
    trait_defs: HashMap<String, TraitInfo>,
    diags: Vec<Diagnostic>,
    loop_depth: usize,
    proc_depth: usize,
    bindings: Vec<HashMap<String, (bool, String)>>,
    steps: usize,
    step_limit: usize,
    limit_reached: bool,
}

impl Analyzer {
    fn new(step_limit: usize) -> Self {
        Self {
            proc_signatures: HashMap::new(),
            struct_defs: HashMap::new(),
            trait_defs: HashMap::new(),
            diags: Vec::new(),
            loop_depth: 0,
            proc_depth: 0,
            bindings: vec![HashMap::new()],
            steps: 0,
            step_limit,
            limit_reached: false,
        }
    }
}

pub fn analyze(program: &Program) -> Vec<Diagnostic> {
    let mut analyzer = Analyzer::new(resolve_analyze_step_limit());
    analyzer.collect_program(program);
    analyzer.walk_program(program);
    analyzer.diags
}

pub fn analyze_with_limit(program: &Program, step_limit: usize) -> Vec<Diagnostic> {
    let mut analyzer = Analyzer::new(step_limit.max(1));
    analyzer.collect_program(program);
    analyzer.walk_program(program);
    analyzer.diags
}

pub fn analyze_typed(program: &Program) -> Result<TypedHirProgram, Vec<Diagnostic>> {
    analyze_typed_with_limit(program, resolve_analyze_step_limit())
}

pub fn analyze_typed_with_limit(
    program: &Program,
    step_limit: usize,
) -> Result<TypedHirProgram, Vec<Diagnostic>> {
    let semantic_diags = analyze_with_limit(program, step_limit);
    if !semantic_diags.is_empty() {
        return Err(semantic_diags);
    }

    let typed = lower_to_typed_hir(program);
    if typed.constraint_errors.is_empty() {
        return Ok(typed);
    }

    let diags = typed
        .constraint_errors
        .iter()
        .map(|err| {
            Diagnostic::new(
                DiagnosticKind::Sema,
                format!("型制約の解決に失敗しました: {}", err.message),
            )
            .with_span(err.span)
            .with_hint("束縛・再束縛・演算に使う値の型整合性を確認してください")
        })
        .collect();
    Err(diags)
}

impl Analyzer {
    fn bump_steps(&mut self, span: crate::common::source::Span) -> bool {
        if self.limit_reached {
            return false;
        }

        self.steps = self.steps.saturating_add(1);
        if self.steps <= self.step_limit {
            return true;
        }

        self.limit_reached = true;
        self.diags.push(
            Diagnostic::new(
                DiagnosticKind::Sema,
                format!(
                    "解析回数が上限を超えたため停止しました（上限: {}）",
                    self.step_limit
                ),
            )
            .with_span(span)
            .with_hint(
                "暫定保護で停止しました。`KOTOBA_ANALYZE_STEP_LIMIT` を必要最小限で調整し、最小再現コードを確認してください",
            ),
        );
        false
    }

    fn collect_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            if self.limit_reached {
                return;
            }
            self.collect_stmt(stmt);
        }
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        if !self.bump_steps(stmt.span) {
            return;
        }

        match &stmt.kind {
            StmtKind::ProcDef {
                name, params, body, ..
            } => {
                self.check_named_params(params);

                let mut seen = HashSet::new();
                for p in params {
                    if !seen.insert(p.particle) {
                        self.diags.push(
                            Diagnostic::new(
                                DiagnosticKind::Sema,
                                format!(
                                    "手順「{}」で助詞「{}」が重複しています。1つの手順内で同じ助詞は1回だけ使えます",
                                    name, p.particle
                                ),
                            )
                            .with_span(p.span)
                            .with_hint("各仮引数に異なる助詞を割り当ててください"),
                        );
                    }
                }

                let sig = ProcSignature {
                    particles: sorted_unique_particles(params.iter().map(|p| p.particle)),
                    particle_count: params.len(),
                };
                self.proc_signatures
                    .entry(name.clone())
                    .or_default()
                    .push(sig);

                self.collect_block(body);
            }
            StmtKind::StructDef { name, fields, .. } => {
                let mut map = HashMap::new();
                for field in fields {
                    map.insert(
                        field.name.clone(),
                        field
                            .type_name
                            .clone()
                            .unwrap_or_else(|| "不明".to_string()),
                    );
                }
                self.struct_defs
                    .insert(name.clone(), StructInfo { fields: map });
            }
            StmtKind::TraitDef { name, methods, .. } => {
                let mut sigs = HashMap::new();
                for method in methods {
                    if let StmtKind::ProcDef {
                        name,
                        params,
                        return_type,
                        ..
                    } = &method.kind
                    {
                        sigs.insert(
                            name.clone(),
                            TraitMethodSig {
                                particles: sorted_unique_particles(
                                    params.iter().map(|p| p.particle),
                                ),
                                return_type: return_type.clone(),
                            },
                        );
                    }
                }
                self.trait_defs
                    .insert(name.clone(), TraitInfo { methods: sigs });
            }
            StmtKind::TraitImpl { methods, .. } => {
                for method in methods {
                    self.collect_stmt(method);
                }
            }
            _ => {}
        }
    }

    fn collect_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            if self.limit_reached {
                return;
            }
            self.collect_stmt(stmt);
        }
    }

    fn walk_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            if self.limit_reached {
                return;
            }
            self.walk_stmt(stmt);
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        if !self.bump_steps(stmt.span) {
            return;
        }

        match &stmt.kind {
            StmtKind::Bind {
                name,
                mutable,
                value,
            } => {
                self.walk_expr(value, false);
                self.check_trait_impl_header_without_body(value, stmt.span);
                let ty = self.infer_expr_type(value);
                self.define_binding(name, *mutable, ty);
            }
            StmtKind::Rebind { value, .. } => self.walk_expr(value, false),
            StmtKind::ExprStmt(expr) => self.walk_expr(expr, false),
            StmtKind::ProcDef { body, .. } => {
                self.proc_depth += 1;
                self.push_scope();
                self.walk_block(body);
                self.pop_scope();
                self.proc_depth = self.proc_depth.saturating_sub(1);
            }
            StmtKind::Return(Some(expr)) => self.walk_expr(expr, false),
            StmtKind::StructDef { methods, .. } | StmtKind::TraitDef { methods, .. } => {
                for m in methods {
                    self.walk_stmt(m);
                }
            }
            StmtKind::TraitImpl {
                type_name,
                trait_name,
                methods,
            } => {
                self.check_trait_impl_semantics(type_name, trait_name, methods, stmt.span);
                for m in methods {
                    self.walk_stmt(m);
                }
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.diags.push(
                        Diagnostic::new(
                            DiagnosticKind::Sema,
                            "「次へ」はループの中でのみ使えます",
                        )
                        .with_span(stmt.span)
                        .with_hint("`N回 繰り返す` / `AからBまで 繰り返す` / `条件 間 繰り返す` の中で使用してください"),
                    );
                }
            }
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    self.diags.push(
                        Diagnostic::new(
                            DiagnosticKind::Sema,
                            "「抜ける」はループの中でのみ使えます",
                        )
                        .with_span(stmt.span)
                        .with_hint("`N回 繰り返す` / `AからBまで 繰り返す` / `条件 間 繰り返す` の中で使用してください"),
                    );
                }
            }
            _ => {}
        }
    }

    fn walk_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            if self.limit_reached {
                return;
            }
            self.walk_stmt(stmt);
        }
    }

    fn walk_expr(&mut self, expr: &Expr, as_property_object: bool) {
        if !self.bump_steps(expr.span) {
            return;
        }

        match &expr.kind {
            ExprKind::KosoAdo(KosoAdoKind::Kore)
            | ExprKind::KosoAdo(KosoAdoKind::Sore)
            | ExprKind::KosoAdo(KosoAdoKind::Are) => {
                if !as_property_object {
                    let name = match &expr.kind {
                        ExprKind::KosoAdo(KosoAdoKind::Kore) => "これ",
                        ExprKind::KosoAdo(KosoAdoKind::Sore) => "それ",
                        ExprKind::KosoAdo(KosoAdoKind::Are) => "あれ",
                        _ => unreachable!(),
                    };
                    self.diags.push(
                        Diagnostic::new(
                            DiagnosticKind::Sema,
                            format!(
                                "DGN-002: {} は単独では使えません。{}の識別子を使用してください",
                                name, name
                            ),
                        )
                        .with_span(expr.span)
                        .with_hint("例: `これの値` / `それの設定` / `あれの設定`"),
                    );
                }
            }
            ExprKind::KosoAdo(KosoAdoKind::Kou) => {
                if self.proc_depth == 0 {
                    self.diags.push(
                        Diagnostic::new(DiagnosticKind::Sema, "「こう」は手順の中でのみ使えます")
                            .with_span(expr.span)
                            .with_hint(
                                "再帰参照が必要な場合は、手順本体の中で `こう` を使ってください",
                            ),
                    );
                }
            }
            ExprKind::PropertyAccess { object, property } => {
                self.walk_expr(object, true);
                self.check_property_access(object, property, expr.span);
            }
            ExprKind::Call { callee, args } => {
                for a in args {
                    self.walk_expr(&a.value, false);
                }
                self.check_call_particle_set(callee, args, expr.span);
                self.check_builtin_call_rules(callee, args, expr.span);
            }
            ExprKind::MethodCall { object, args, .. } => {
                self.walk_expr(object, false);
                for a in args {
                    self.walk_expr(&a.value, false);
                }
            }
            ExprKind::BinaryOp { left, right, .. } => {
                self.walk_expr(left, false);
                self.walk_expr(right, false);
                self.check_counter_dimension_compat(left, right, expr.span);
            }
            ExprKind::Comparison { left, right, .. } => {
                self.walk_expr(left, false);
                self.walk_expr(right, false);
                self.check_counter_dimension_compat(left, right, expr.span);
            }
            ExprKind::Logical { left, right, .. } => {
                self.walk_expr(left, false);
                self.walk_expr(right, false);
            }
            ExprKind::UnaryOp { operand, .. }
            | ExprKind::WithCounter { value: operand, .. }
            | ExprKind::Throw(operand) => self.walk_expr(operand, false),
            ExprKind::If {
                condition,
                then_block,
                elif_clauses,
                else_block,
            } => {
                self.walk_expr(condition, false);
                self.walk_block(then_block);
                for (c, b) in elif_clauses {
                    self.walk_expr(c, false);
                    self.walk_block(b);
                }
                if let Some(b) = else_block {
                    self.walk_block(b);
                }
            }
            ExprKind::Loop(kind) => match kind.as_ref() {
                LoopKind::Times { count, body, .. } => {
                    self.walk_expr(count, false);
                    self.walk_loop_block(body);
                }
                LoopKind::Range { from, to, body, .. } => {
                    self.walk_expr(from, false);
                    self.walk_expr(to, false);
                    self.walk_loop_block(body);
                }
                LoopKind::While { condition, body } => {
                    self.walk_expr(condition, false);
                    self.walk_loop_block(body);
                }
                LoopKind::ForEach { iterable, body, .. } => {
                    self.walk_expr(iterable, false);
                    self.walk_loop_block(body);
                }
            },
            ExprKind::Lambda { params, body } => {
                self.check_named_params(params);
                self.walk_block(body);
            }
            ExprKind::TryCatch {
                body,
                catch_body,
                finally_body,
                ..
            } => {
                self.walk_block(body);
                if let Some(b) = catch_body {
                    self.walk_block(b);
                }
                if let Some(b) = finally_body {
                    self.walk_block(b);
                }
            }
            ExprKind::List(items) => {
                for i in items {
                    self.walk_expr(i, false);
                }
            }
            ExprKind::Map(entries) => {
                for (_, v) in entries {
                    self.walk_expr(v, false);
                }
            }
            ExprKind::StringInterp(parts) => {
                for p in parts {
                    if let StringPart::Expr(e) = p {
                        self.walk_expr(e, false);
                    }
                }
            }
            ExprKind::Match { target, arms } => {
                self.walk_expr(target, false);
                for arm in arms {
                    self.walk_block(&arm.body);
                }
            }
            ExprKind::TeChain { steps } => {
                for step in steps {
                    match step {
                        ChainStep::Call { args, .. } => {
                            for a in args {
                                self.walk_expr(&a.value, false);
                            }
                        }
                        ChainStep::Branch { if_expr } => self.walk_expr(if_expr, false),
                    }
                }
            }
            ExprKind::BranchChain { if_expr } => self.walk_expr(if_expr, false),
            ExprKind::Construct { type_name, fields } => {
                for (_, v) in fields {
                    self.walk_expr(v, false);
                }
                self.check_construct_semantics(type_name, fields, expr.span);
            }
            ExprKind::Destructure { value, .. } => self.walk_expr(value, false),
            _ => {}
        }
    }

    fn walk_loop_block(&mut self, block: &Block) {
        if self.limit_reached {
            return;
        }
        self.loop_depth += 1;
        self.push_scope();
        self.walk_block(block);
        self.pop_scope();
        self.loop_depth = self.loop_depth.saturating_sub(1);
    }

    fn push_scope(&mut self) {
        self.bindings.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.bindings.len() > 1 {
            self.bindings.pop();
        }
    }

    fn define_binding(&mut self, name: &str, mutable: bool, ty: String) {
        if let Some(scope) = self.bindings.last_mut() {
            scope.insert(name.to_string(), (mutable, ty));
        }
    }

    fn lookup_binding(&self, name: &str) -> Option<&(bool, String)> {
        self.bindings.iter().rev().find_map(|scope| scope.get(name))
    }

    fn infer_expr_type(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Integer(_) | ExprKind::Float(_) => "数".to_string(),
            ExprKind::StringLiteral(_) | ExprKind::StringInterp(_) => "文字列".to_string(),
            ExprKind::Bool(_) => "真偽".to_string(),
            ExprKind::None => "無".to_string(),
            ExprKind::Identifier(name) => self
                .lookup_binding(name)
                .map(|(_, ty)| ty.clone())
                .unwrap_or_else(|| "不明".to_string()),
            ExprKind::Construct { type_name, .. } => type_name.clone(),
            ExprKind::PropertyAccess { object, property } => self
                .property_type_of(object, property)
                .unwrap_or_else(|| "不明".to_string()),
            _ => "不明".to_string(),
        }
    }

    fn property_type_of(&self, object: &Expr, property: &str) -> Option<String> {
        let object_ty = self.infer_expr_type(object);
        self.struct_defs
            .get(&object_ty)
            .and_then(|info| info.fields.get(property).cloned())
    }

    fn check_construct_semantics(
        &mut self,
        type_name: &str,
        fields: &[(String, Expr)],
        span: crate::common::source::Span,
    ) {
        let Some(info) = self.struct_defs.get(type_name).cloned() else {
            return;
        };

        let mut seen = HashSet::new();
        for (field_name, value_expr) in fields {
            seen.insert(field_name.clone());
            let Some(expected_ty) = info.fields.get(field_name) else {
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        format!(
                            "DGN-012: 「{}」には「{}」フィールドがありません",
                            type_name, field_name
                        ),
                    )
                    .with_span(value_expr.span)
                    .with_hint("定義済みフィールド名を指定してください"),
                );
                continue;
            };

            let actual_ty = self.infer_expr_type(value_expr);
            if actual_ty != "不明" && actual_ty != *expected_ty {
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        format!(
                            "DGN-011: 「{}」には「{}」が必要ですが「{}」が渡されました",
                            field_name, expected_ty, actual_ty
                        ),
                    )
                    .with_span(value_expr.span)
                    .with_hint("フィールド宣言と同じ型の値を指定してください"),
                );
            }
        }

        for field_name in info.fields.keys() {
            if !seen.contains(field_name) {
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        format!(
                            "DGN-010: {}を作るには「{}」フィールドの指定が必要です",
                            type_name, field_name
                        ),
                    )
                    .with_span(span)
                    .with_hint("不足しているフィールドを `【名前: 値】` に追加してください"),
                );
            }
        }
    }

    fn check_property_access(
        &mut self,
        object: &Expr,
        property: &str,
        span: crate::common::source::Span,
    ) {
        let object_ty = self.infer_expr_type(object);
        let Some(info) = self.struct_defs.get(&object_ty) else {
            return;
        };
        if info.fields.contains_key(property) {
            return;
        }
        self.diags.push(
            Diagnostic::new(
                DiagnosticKind::Sema,
                format!(
                    "DGN-012: 「{}」には「{}」フィールドがありません",
                    object_ty, property
                ),
            )
            .with_span(span)
            .with_hint("存在するフィールド名を確認してください"),
        );
    }

    fn check_field_rebind_mutability(&mut self, target: &Expr, span: crate::common::source::Span) {
        let ExprKind::PropertyAccess { object, .. } = &target.kind else {
            return;
        };
        let Some(root_name) = root_identifier(object) else {
            return;
        };
        let Some((mutable, _)) = self.lookup_binding(root_name) else {
            return;
        };
        if *mutable {
            return;
        }
        self.diags.push(
            Diagnostic::new(
                DiagnosticKind::Sema,
                format!(
                    "DGN-015: 「{}」は不変束縛のためフィールドを変えることができません",
                    root_name
                ),
            )
            .with_span(span)
            .with_hint("`変わる` を付けて可変束縛にしてください"),
        );
    }

    fn check_trait_impl_semantics(
        &mut self,
        type_name: &str,
        trait_name: &str,
        methods: &[Stmt],
        span: crate::common::source::Span,
    ) {
        let Some(trait_info) = self.trait_defs.get(trait_name) else {
            return;
        };

        let mut impl_methods: HashMap<String, TraitMethodSig> = HashMap::new();
        for method in methods {
            if let StmtKind::ProcDef {
                name,
                params,
                return_type,
                ..
            } = &method.kind
            {
                impl_methods.insert(
                    name.clone(),
                    TraitMethodSig {
                        particles: sorted_unique_particles(params.iter().map(|p| p.particle)),
                        return_type: return_type.clone(),
                    },
                );
            }
        }

        for (required_name, required_sig) in &trait_info.methods {
            let Some(actual_sig) = impl_methods.get(required_name) else {
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        format!(
                            "DGN-013: 「{}」は「{}」を実装していますが「{}」が定義されていません",
                            type_name, trait_name, required_name
                        ),
                    )
                    .with_span(span)
                    .with_hint("特性で宣言されたメソッドをすべて実装してください"),
                );
                continue;
            };

            if required_sig.particles != actual_sig.particles
                || required_sig.return_type != actual_sig.return_type
            {
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        format!(
                            "DGN-014: 「{}」のシグネチャが特性「{}」の宣言と一致しません",
                            required_name, trait_name
                        ),
                    )
                    .with_span(span)
                    .with_hint("助詞と戻り型を特性宣言と一致させてください"),
                );
            }
        }
    }

    fn check_named_params(&mut self, params: &[Param]) {
        for p in params {
            if p.name.is_none() {
                self.diags.push(
                    Diagnostic::new(DiagnosticKind::Sema, "無名仮引数は v1 で禁止されています")
                        .with_span(p.span)
                        .with_hint("`【:を】` ではなく `【名前:を】` を使用してください"),
                );
            }
        }
    }

    fn check_call_particle_set(
        &mut self,
        callee: &str,
        args: &[ParticleArg],
        span: crate::common::source::Span,
    ) {
        let Some(candidates) = self.proc_signatures.get(callee) else {
            return;
        };

        let actual_particles = sorted_unique_particles(args.iter().map(|a| a.particle));
        let actual_count = args.len();
        let matched = candidates
            .iter()
            .any(|sig| sig.particle_count == actual_count && sig.particles == actual_particles);

        if matched {
            return;
        }

        let expected = candidates
            .iter()
            .map(|sig| format_particle_set(&sig.particles))
            .collect::<Vec<_>>()
            .join(" / ");
        let actual = format_particle_set(&actual_particles);

        self.diags.push(
            Diagnostic::new(
                DiagnosticKind::Sema,
                format!(
                    "DGN-003: 手順「{}」の助詞セットが一致しません。要求: {} / 実引数: {}",
                    callee, expected, actual
                ),
            )
            .with_span(span)
            .with_hint("定義側の助詞集合と実引数の助詞集合を完全一致させてください"),
        );
    }

    fn check_counter_dimension_compat(
        &mut self,
        left: &Expr,
        right: &Expr,
        span: crate::common::source::Span,
    ) {
        let left_dim = infer_dimension(left);
        let right_dim = infer_dimension(right);
        let (Some(ld), Some(rd)) = (left_dim, right_dim) else {
            return;
        };
        if ld == rd {
            return;
        }

        self.diags.push(
            Diagnostic::new(
                DiagnosticKind::Sema,
                format!(
                    "DGN-004: 助数詞次元が一致しません。左辺: {} / 右辺: {}",
                    ld, rd
                ),
            )
            .with_span(span)
            .with_hint("同じ助数詞次元の値同士で演算・比較してください"),
        );
    }

    fn check_builtin_call_rules(
        &mut self,
        callee: &str,
        args: &[ParticleArg],
        span: crate::common::source::Span,
    ) {
        match callee {
            "変える" => {
                let has_wo = args.iter().any(|a| a.particle == Particle::Wo);
                let has_ni = args.iter().any(|a| a.particle == Particle::Ni);
                if args.len() == 2 && has_wo && has_ni {
                    if let Some(target) = args.iter().find(|a| a.particle == Particle::Wo) {
                        self.check_field_rebind_mutability(&target.value, span);
                    }
                    return;
                }
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        "「変える」は `対象を 新しい値に 変える` の形で指定してください",
                    )
                    .with_span(span)
                    .with_hint("例: `数を 1に 変える`"),
                );
            }
            "入力する" => {
                if args.is_empty() {
                    return;
                }
                self.diags.push(
                    Diagnostic::new(DiagnosticKind::Sema, "「入力する」は引数を取りません")
                        .with_span(span)
                        .with_hint("例: `名前 は 入力する`"),
                );
            }
            "読む" => {
                let has_wo = args.iter().any(|a| a.particle == Particle::Wo);
                if args.len() == 1 && has_wo {
                    return;
                }
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        "「読む」は `「ファイル」を 読む` の形で指定してください",
                    )
                    .with_span(span)
                    .with_hint("例: `内容 は 「data.txt」を 読む`"),
                );
            }
            "書く" => {
                let has_wo = args.iter().any(|a| a.particle == Particle::Wo);
                let has_ni = args.iter().any(|a| a.particle == Particle::Ni);
                if args.len() == 2 && has_wo && has_ni {
                    return;
                }
                self.diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        "「書く」は `「内容」を 「ファイル」に 書く` の形で指定してください",
                    )
                    .with_span(span)
                    .with_hint("例: `「こんにちは」を 「out.txt」に 書く`"),
                );
            }
            _ => {}
        }
    }

    fn check_trait_impl_header_without_body(
        &mut self,
        value: &Expr,
        span: crate::common::source::Span,
    ) {
        let ExprKind::Call { callee, args } = &value.kind else {
            return;
        };
        if callee != "持つ" && callee != "を持つ" {
            return;
        }
        if args.len() != 1 || args[0].particle != Particle::Wo {
            return;
        }
        if !matches!(args[0].value.kind, ExprKind::Identifier(_)) {
            return;
        }

        self.diags.push(
            Diagnostic::new(DiagnosticKind::Sema, "特性実装には本体ブロックが必要です")
                .with_span(span)
                .with_hint("`人は 表示できる を持つ` の後にインデントした本体を記述してください"),
        );
    }
}

fn resolve_analyze_step_limit() -> usize {
    std::env::var("KOTOBA_ANALYZE_STEP_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_ANALYZE_STEP_LIMIT)
}

fn root_identifier(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name),
        ExprKind::PropertyAccess { object, .. } => root_identifier(object),
        _ => None,
    }
}

fn sorted_unique_particles<I>(iter: I) -> Vec<Particle>
where
    I: IntoIterator<Item = Particle>,
{
    let mut particles: Vec<Particle> = iter.into_iter().collect();
    particles.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    particles.dedup();
    particles
}

fn format_particle_set(particles: &[Particle]) -> String {
    if particles.is_empty() {
        return "{}".to_string();
    }
    let joined = particles
        .iter()
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join("、");
    format!("{{{joined}}}")
}

fn infer_dimension(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::WithCounter { counter, .. } => Some(counter.clone()),
        ExprKind::BinaryOp { left, right, .. } | ExprKind::Comparison { left, right, .. } => {
            let left_dim = infer_dimension(left);
            let right_dim = infer_dimension(right);
            match (left_dim, right_dim) {
                (Some(l), Some(r)) if l == r => Some(l),
                (Some(l), None) => Some(l),
                (None, Some(r)) => Some(r),
                _ => None,
            }
        }
        ExprKind::UnaryOp { operand, .. } => infer_dimension(operand),
        _ => None,
    }
}
