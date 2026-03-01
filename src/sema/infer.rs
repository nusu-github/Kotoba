use crate::diag::report::{Diagnostic, DiagnosticKind};
use crate::frontend::ast::{Block, Expr, ExprKind, KosoAdoKind, Program, Stmt, StmtKind};

pub fn analyze(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in &program.statements {
        walk_stmt(stmt, &mut diags);
    }
    diags
}

fn walk_stmt(stmt: &Stmt, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Bind { value, .. } => walk_expr(value, diags, false),
        StmtKind::Rebind { value, .. } => walk_expr(value, diags, false),
        StmtKind::ExprStmt(expr) => walk_expr(expr, diags, false),
        StmtKind::ProcDef { params, body, .. } => {
            for p in params {
                if p.name.is_none() {
                    diags.push(
                        Diagnostic::new(DiagnosticKind::Sema, "無名仮引数は v1 で禁止されています")
                            .with_span(p.span)
                            .with_hint("`【:を】` ではなく `【名前:を】` を使用してください"),
                    );
                }
            }
            walk_block(body, diags);
        }
        StmtKind::Return(Some(expr)) => walk_expr(expr, diags, false),
        StmtKind::StructDef { methods, .. } | StmtKind::TraitDef { methods, .. } => {
            for m in methods {
                walk_stmt(m, diags);
            }
        }
        StmtKind::TraitImpl { methods, .. } => {
            for m in methods {
                walk_stmt(m, diags);
            }
        }
        _ => {}
    }
}

fn walk_block(block: &Block, diags: &mut Vec<Diagnostic>) {
    for stmt in &block.statements {
        walk_stmt(stmt, diags);
    }
}

fn walk_expr(expr: &Expr, diags: &mut Vec<Diagnostic>, as_property_object: bool) {
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
                diags.push(
                    Diagnostic::new(
                        DiagnosticKind::Sema,
                        format!("{} は単独では使えません。{}の識別子 を使用してください", name, name),
                    )
                    .with_span(expr.span)
                    .with_hint("例: `これの値` / `それの設定` / `あれの設定`"),
                );
            }
        }
        ExprKind::PropertyAccess { object, .. } => {
            walk_expr(object, diags, true);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                walk_expr(&a.value, diags, false);
            }
        }
        ExprKind::MethodCall { object, args, .. } => {
            walk_expr(object, diags, false);
            for a in args {
                walk_expr(&a.value, diags, false);
            }
        }
        ExprKind::BinaryOp { left, right, .. }
        | ExprKind::Comparison { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            walk_expr(left, diags, false);
            walk_expr(right, diags, false);
        }
        ExprKind::UnaryOp { operand, .. }
        | ExprKind::WithCounter { value: operand, .. }
        | ExprKind::Throw(operand) => walk_expr(operand, diags, false),
        ExprKind::If {
            condition,
            then_block,
            elif_clauses,
            else_block,
        } => {
            walk_expr(condition, diags, false);
            walk_block(then_block, diags);
            for (c, b) in elif_clauses {
                walk_expr(c, diags, false);
                walk_block(b, diags);
            }
            if let Some(b) = else_block {
                walk_block(b, diags);
            }
        }
        ExprKind::Loop(kind) => match kind.as_ref() {
            crate::frontend::ast::LoopKind::Times { count, body, .. } => {
                walk_expr(count, diags, false);
                walk_block(body, diags);
            }
            crate::frontend::ast::LoopKind::Range { from, to, body, .. } => {
                walk_expr(from, diags, false);
                walk_expr(to, diags, false);
                walk_block(body, diags);
            }
            crate::frontend::ast::LoopKind::While { condition, body } => {
                walk_expr(condition, diags, false);
                walk_block(body, diags);
            }
            crate::frontend::ast::LoopKind::ForEach { iterable, body, .. } => {
                walk_expr(iterable, diags, false);
                walk_block(body, diags);
            }
        },
        ExprKind::Lambda { body, .. } => walk_block(body, diags),
        ExprKind::TryCatch {
            body,
            catch_body,
            finally_body,
            ..
        } => {
            walk_block(body, diags);
            if let Some(b) = catch_body {
                walk_block(b, diags);
            }
            if let Some(b) = finally_body {
                walk_block(b, diags);
            }
        }
        ExprKind::List(items) => {
            for i in items {
                walk_expr(i, diags, false);
            }
        }
        ExprKind::Map(entries) => {
            for (_, v) in entries {
                walk_expr(v, diags, false);
            }
        }
        ExprKind::StringInterp(parts) => {
            for p in parts {
                if let crate::frontend::ast::StringPart::Expr(e) = p {
                    walk_expr(e, diags, false);
                }
            }
        }
        ExprKind::Match { target, arms } => {
            walk_expr(target, diags, false);
            for arm in arms {
                walk_block(&arm.body, diags);
            }
        }
        ExprKind::TeChain { steps } => {
            for step in steps {
                match step {
                    crate::frontend::ast::ChainStep::Call { args, .. } => {
                        for a in args {
                            walk_expr(&a.value, diags, false);
                        }
                    }
                    crate::frontend::ast::ChainStep::Branch { if_expr } => {
                        walk_expr(if_expr, diags, false)
                    }
                }
            }
        }
        ExprKind::BranchChain { if_expr } => walk_expr(if_expr, diags, false),
        ExprKind::Construct { fields, .. } => {
            for (_, v) in fields {
                walk_expr(v, diags, false);
            }
        }
        ExprKind::Destructure { value, .. } => walk_expr(value, diags, false),
        _ => {}
    }
}
