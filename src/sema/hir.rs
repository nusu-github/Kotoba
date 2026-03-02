use crate::common::source::Span;
use crate::frontend::ast::{
    Block, ChainStep, Expr, ExprKind, LogicalOp, LoopKind, Program, Stmt, StmtKind, StringPart,
};
use crate::sema::symbols::SymbolTable;
use crate::sema::types::{ConstraintError, ConstraintSolver, Type, TypeConstraint, TypeVarId};

#[derive(Debug, Clone)]
pub struct TypedExprInfo {
    pub span: Span,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct TypedHirProgram {
    pub program: Program,
    pub expr_types: Vec<TypedExprInfo>,
    pub constraints: Vec<TypeConstraint>,
    pub constraint_errors: Vec<ConstraintError>,
    pub symbols: SymbolTable,
}

impl TypedHirProgram {
    pub fn type_at(&self, span: Span) -> Option<&Type> {
        self.expr_types
            .iter()
            .find_map(|info| (info.span == span).then_some(&info.ty))
    }
}

pub fn lower_to_typed_hir(program: &Program) -> TypedHirProgram {
    let mut builder = TypedHirBuilder::new();
    builder.visit_program(program);

    let mut solver = ConstraintSolver::new();
    let constraint_errors = solver.solve(&builder.constraints);

    for info in &mut builder.expr_types {
        info.ty = solver.resolve(&info.ty);
    }
    builder
        .symbols
        .apply_type_substitutions(|ty| solver.resolve(ty));

    TypedHirProgram {
        program: program.clone(),
        expr_types: builder.expr_types,
        constraints: builder.constraints,
        constraint_errors,
        symbols: builder.symbols,
    }
}

struct TypedHirBuilder {
    expr_types: Vec<TypedExprInfo>,
    constraints: Vec<TypeConstraint>,
    symbols: SymbolTable,
    next_type_var: u32,
}

impl TypedHirBuilder {
    fn new() -> Self {
        Self {
            expr_types: Vec::new(),
            constraints: Vec::new(),
            symbols: SymbolTable::new(),
            next_type_var: 0,
        }
    }

    fn fresh_type_var(&mut self) -> Type {
        let id = self.next_type_var;
        self.next_type_var = self.next_type_var.saturating_add(1);
        Type::Var(TypeVarId(id))
    }

    fn note_expr(&mut self, span: Span, ty: Type) -> Type {
        self.expr_types.push(TypedExprInfo {
            span,
            ty: ty.clone(),
        });
        ty
    }

    fn visit_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.visit_stmt(stmt);
        }
    }

    fn visit_block(&mut self, block: &Block) {
        self.symbols.push_scope();
        for stmt in &block.statements {
            self.visit_stmt(stmt);
        }
        self.symbols.pop_scope();
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Bind {
                name,
                mutable,
                value,
            } => {
                let ty = self.visit_expr(value);
                self.symbols.define(name.clone(), ty, *mutable, stmt.span);
            }
            StmtKind::Rebind { name, value } => {
                let ty = self.visit_expr(value);
                if let Some(existing) = self.symbols.lookup(name) {
                    self.constraints.push(TypeConstraint::Equal {
                        left: existing.ty.clone(),
                        right: ty.clone(),
                        span: stmt.span,
                    });
                }
                self.symbols.assign(name, ty, stmt.span);
            }
            StmtKind::ExprStmt(expr) => {
                self.visit_expr(expr);
            }
            StmtKind::ProcDef {
                name, params, body, ..
            } => {
                self.symbols
                    .define(name.clone(), Type::Procedure, false, stmt.span);
                self.symbols.push_scope();
                for param in params {
                    let param_name = param
                        .name
                        .clone()
                        .unwrap_or_else(|| param.particle.as_str().to_string());
                    let param_ty = self.fresh_type_var();
                    self.symbols.define(param_name, param_ty, true, param.span);
                }
                for body_stmt in &body.statements {
                    self.visit_stmt(body_stmt);
                }
                self.symbols.pop_scope();
            }
            StmtKind::StructDef { name, methods, .. } => {
                self.symbols
                    .define(name.clone(), Type::Map, false, stmt.span);
                for method in methods {
                    self.visit_stmt(method);
                }
            }
            StmtKind::TraitDef { name, methods, .. } => {
                self.symbols
                    .define(name.clone(), Type::Unknown, false, stmt.span);
                for method in methods {
                    self.visit_stmt(method);
                }
            }
            StmtKind::TraitImpl { methods, .. } => {
                for method in methods {
                    self.visit_stmt(method);
                }
            }
            StmtKind::Use { .. } | StmtKind::Continue | StmtKind::Break => {}
            StmtKind::Return(value) => {
                if let Some(expr) = value {
                    self.visit_expr(expr);
                }
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr) -> Type {
        let ty = match &expr.kind {
            ExprKind::Integer(_) | ExprKind::Float(_) => Type::Number,
            ExprKind::StringLiteral(_) => Type::String,
            ExprKind::StringInterp(parts) => {
                for part in parts {
                    if let StringPart::Expr(inner) = part {
                        self.visit_expr(inner);
                    }
                }
                Type::String
            }
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::None => Type::None,
            ExprKind::Identifier(name) => self
                .symbols
                .lookup(name)
                .map(|symbol| symbol.ty.clone())
                .unwrap_or(Type::Unknown),
            ExprKind::KosoAdo(_) => Type::Unknown,
            ExprKind::List(items) => {
                for item in items {
                    self.visit_expr(item);
                }
                Type::List
            }
            ExprKind::Map(entries) => {
                for (_, value) in entries {
                    self.visit_expr(value);
                }
                Type::Map
            }
            ExprKind::Call { callee, args } => {
                let arg_types = args
                    .iter()
                    .map(|arg| self.visit_expr(&arg.value))
                    .collect::<Vec<_>>();
                match callee.as_str() {
                    "表示する" => Type::None,
                    "変える" => {
                        if let [target_ty, value_ty] = arg_types.as_slice() {
                            self.constraints.push(TypeConstraint::Equal {
                                left: target_ty.clone(),
                                right: value_ty.clone(),
                                span: expr.span,
                            });
                        }
                        Type::None
                    }
                    _ => self.fresh_type_var(),
                }
            }
            ExprKind::PropertyAccess { object, .. } => {
                self.visit_expr(object);
                self.fresh_type_var()
            }
            ExprKind::MethodCall { object, args, .. } => {
                self.visit_expr(object);
                for arg in args {
                    self.visit_expr(&arg.value);
                }
                self.fresh_type_var()
            }
            ExprKind::BinaryOp { left, right, .. } => {
                let left_ty = self.visit_expr(left);
                let right_ty = self.visit_expr(right);
                self.constraints.push(TypeConstraint::SameDimension {
                    left: left_ty.clone(),
                    right: right_ty.clone(),
                    span: expr.span,
                });
                merge_numeric_type(left_ty, right_ty)
            }
            ExprKind::UnaryOp { operand, .. } => {
                self.visit_expr(operand);
                Type::Bool
            }
            ExprKind::Comparison { left, right, .. } => {
                let left_ty = self.visit_expr(left);
                let right_ty = self.visit_expr(right);
                self.constraints.push(TypeConstraint::SameDimension {
                    left: left_ty,
                    right: right_ty,
                    span: expr.span,
                });
                Type::Bool
            }
            ExprKind::Logical { left, right, op } => {
                self.visit_expr(left);
                self.visit_expr(right);
                match op {
                    LogicalOp::And | LogicalOp::Or => Type::Bool,
                }
            }
            ExprKind::If {
                condition,
                then_block,
                elif_clauses,
                else_block,
            } => {
                self.visit_expr(condition);
                self.visit_block(then_block);
                for (elif_cond, elif_block) in elif_clauses {
                    self.visit_expr(elif_cond);
                    self.visit_block(elif_block);
                }
                if let Some(block) = else_block {
                    self.visit_block(block);
                }
                self.fresh_type_var()
            }
            ExprKind::Match { target, arms } => {
                self.visit_expr(target);
                for arm in arms {
                    self.visit_block(&arm.body);
                }
                self.fresh_type_var()
            }
            ExprKind::Loop(kind) => {
                match kind.as_ref() {
                    LoopKind::Times { count, body, .. } => {
                        self.visit_expr(count);
                        self.visit_block(body);
                    }
                    LoopKind::Range { from, to, body, .. } => {
                        self.visit_expr(from);
                        self.visit_expr(to);
                        self.visit_block(body);
                    }
                    LoopKind::While { condition, body } => {
                        self.visit_expr(condition);
                        self.visit_block(body);
                    }
                    LoopKind::ForEach { iterable, body, .. } => {
                        self.visit_expr(iterable);
                        self.visit_block(body);
                    }
                }
                Type::None
            }
            ExprKind::TeChain { steps } => {
                for step in steps {
                    match step {
                        ChainStep::Call { args, .. } => {
                            for arg in args {
                                self.visit_expr(&arg.value);
                            }
                        }
                        ChainStep::Branch { if_expr } => {
                            self.visit_expr(if_expr);
                        }
                    }
                }
                self.fresh_type_var()
            }
            ExprKind::BranchChain { if_expr } => {
                self.visit_expr(if_expr);
                self.fresh_type_var()
            }
            ExprKind::Lambda { params, body } => {
                self.symbols.push_scope();
                for param in params {
                    let param_name = param
                        .name
                        .clone()
                        .unwrap_or_else(|| param.particle.as_str().to_string());
                    let param_ty = self.fresh_type_var();
                    self.symbols.define(param_name, param_ty, true, param.span);
                }
                self.visit_block(body);
                self.symbols.pop_scope();
                Type::Procedure
            }
            ExprKind::TryCatch {
                body,
                catch_param,
                catch_body,
                finally_body,
            } => {
                self.visit_block(body);
                if let Some(param) = catch_param {
                    self.symbols.push_scope();
                    let catch_ty = self.fresh_type_var();
                    self.symbols
                        .define(param.clone(), catch_ty, true, expr.span);
                    if let Some(catch) = catch_body {
                        self.visit_block(catch);
                    }
                    self.symbols.pop_scope();
                } else if let Some(catch) = catch_body {
                    self.visit_block(catch);
                }
                if let Some(finally) = finally_body {
                    self.visit_block(finally);
                }
                self.fresh_type_var()
            }
            ExprKind::Throw(inner) => {
                self.visit_expr(inner);
                Type::None
            }
            ExprKind::WithCounter { value, counter } => {
                self.visit_expr(value);
                Type::NumberWithDimension(counter.clone())
            }
            ExprKind::Construct { fields, .. } => {
                for (_, value) in fields {
                    self.visit_expr(value);
                }
                Type::Map
            }
            ExprKind::Destructure { value, .. } => {
                self.visit_expr(value);
                self.fresh_type_var()
            }
        };
        self.note_expr(expr.span, ty)
    }
}

fn merge_numeric_type(left: Type, right: Type) -> Type {
    match (left, right) {
        (Type::NumberWithDimension(ld), Type::NumberWithDimension(rd)) if ld == rd => {
            Type::NumberWithDimension(ld)
        }
        (Type::NumberWithDimension(dim), Type::Number)
        | (Type::Number, Type::NumberWithDimension(dim)) => Type::NumberWithDimension(dim),
        (Type::Number, Type::Number) => Type::Number,
        (Type::Unknown, other) | (other, Type::Unknown) => other,
        (Type::Var(_), other) | (other, Type::Var(_)) => other,
        _ => Type::Unknown,
    }
}
