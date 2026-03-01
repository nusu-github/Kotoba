use std::collections::{HashMap, HashSet};

use crate::diag::report::{Diagnostic, DiagnosticKind};
use crate::frontend::ast::{
    Block, ChainStep, Expr, ExprKind, KosoAdoKind, LoopKind, Param, ParticleArg, Program, Stmt,
    StmtKind, StringPart,
};
use crate::frontend::token::Particle;

#[derive(Debug, Clone)]
struct ProcSignature {
    particles: Vec<Particle>,
    particle_count: usize,
}

#[derive(Debug, Default)]
struct Analyzer {
    proc_signatures: HashMap<String, Vec<ProcSignature>>,
    diags: Vec<Diagnostic>,
    loop_depth: usize,
}

pub fn analyze(program: &Program) -> Vec<Diagnostic> {
    let mut analyzer = Analyzer::default();
    analyzer.collect_program(program);
    analyzer.walk_program(program);
    analyzer.diags
}

impl Analyzer {
    fn collect_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.collect_stmt(stmt);
        }
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
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
            StmtKind::StructDef { methods, .. } | StmtKind::TraitDef { methods, .. } => {
                for method in methods {
                    self.collect_stmt(method);
                }
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
            self.collect_stmt(stmt);
        }
    }

    fn walk_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.walk_stmt(stmt);
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Bind { value, .. } => self.walk_expr(value, false),
            StmtKind::Rebind { value, .. } => self.walk_expr(value, false),
            StmtKind::ExprStmt(expr) => self.walk_expr(expr, false),
            StmtKind::ProcDef { body, .. } => {
                self.walk_block(body);
            }
            StmtKind::Return(Some(expr)) => self.walk_expr(expr, false),
            StmtKind::StructDef { methods, .. } | StmtKind::TraitDef { methods, .. } => {
                for m in methods {
                    self.walk_stmt(m);
                }
            }
            StmtKind::TraitImpl { methods, .. } => {
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
            self.walk_stmt(stmt);
        }
    }

    fn walk_expr(&mut self, expr: &Expr, as_property_object: bool) {
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
                                "{} は単独では使えません。{}の識別子 を使用してください",
                                name, name
                            ),
                        )
                        .with_span(expr.span)
                        .with_hint("例: `これの値` / `それの設定` / `あれの設定`"),
                    );
                }
            }
            ExprKind::PropertyAccess { object, .. } => {
                self.walk_expr(object, true);
            }
            ExprKind::Call { callee, args } => {
                for a in args {
                    self.walk_expr(&a.value, false);
                }
                self.check_call_particle_set(callee, args, expr.span);
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
            ExprKind::Construct { fields, .. } => {
                for (_, v) in fields {
                    self.walk_expr(v, false);
                }
            }
            ExprKind::Destructure { value, .. } => self.walk_expr(value, false),
            _ => {}
        }
    }

    fn walk_loop_block(&mut self, block: &Block) {
        self.loop_depth += 1;
        self.walk_block(block);
        self.loop_depth = self.loop_depth.saturating_sub(1);
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
