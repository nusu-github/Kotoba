use std::collections::{HashMap, HashSet};

use crate::common::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVarId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Number,
    String,
    Bool,
    None,
    Procedure,
    List,
    Map,
    NumberWithDimension(String),
    Var(TypeVarId),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeConstraint {
    Equal { left: Type, right: Type, span: Span },
    SameDimension { left: Type, right: Type, span: Span },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintError {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct ConstraintSolver {
    substitutions: HashMap<TypeVarId, Type>,
}

impl ConstraintSolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn solve(&mut self, constraints: &[TypeConstraint]) -> Vec<ConstraintError> {
        let mut errors = Vec::new();
        for constraint in constraints {
            match constraint {
                TypeConstraint::Equal { left, right, span } => {
                    self.unify(left.clone(), right.clone(), *span, &mut errors);
                }
                TypeConstraint::SameDimension { left, right, span } => {
                    self.unify_same_dimension(left.clone(), right.clone(), *span, &mut errors);
                }
            }
        }
        errors
    }

    pub fn resolve(&self, ty: &Type) -> Type {
        let mut current = ty.clone();
        let mut visited = HashSet::new();
        loop {
            match current {
                Type::Var(id) => {
                    if !visited.insert(id) {
                        return Type::Unknown;
                    }
                    if let Some(next) = self.substitutions.get(&id) {
                        current = next.clone();
                    } else {
                        return Type::Var(id);
                    }
                }
                _ => return current,
            }
        }
    }

    fn unify(&mut self, left: Type, right: Type, span: Span, errors: &mut Vec<ConstraintError>) {
        let left = self.resolve(&left);
        let right = self.resolve(&right);
        if left == right {
            return;
        }

        match (left, right) {
            (Type::Var(left_id), Type::Var(right_id)) => {
                let (from, to) = if left_id.0 > right_id.0 {
                    (left_id, right_id)
                } else {
                    (right_id, left_id)
                };
                self.substitutions.insert(from, Type::Var(to));
            }
            (Type::Var(id), other) | (other, Type::Var(id)) => {
                self.substitutions.insert(id, other);
            }
            (Type::Unknown, _) | (_, Type::Unknown) => {}
            (Type::Number, Type::NumberWithDimension(_))
            | (Type::NumberWithDimension(_), Type::Number) => {}
            (Type::NumberWithDimension(ld), Type::NumberWithDimension(rd)) => {
                if ld != rd {
                    errors.push(ConstraintError {
                        message: format!("助数詞次元が一致しません。左辺: {} / 右辺: {}", ld, rd),
                        span,
                    });
                }
            }
            (l, r) => {
                errors.push(ConstraintError {
                    message: format!("型が一致しません。左辺: {l:?} / 右辺: {r:?}"),
                    span,
                });
            }
        }
    }

    fn unify_same_dimension(
        &mut self,
        left: Type,
        right: Type,
        span: Span,
        errors: &mut Vec<ConstraintError>,
    ) {
        let left = self.resolve(&left);
        let right = self.resolve(&right);
        match (left, right) {
            (Type::Var(left_id), Type::Var(right_id)) => {
                let (from, to) = if left_id.0 > right_id.0 {
                    (left_id, right_id)
                } else {
                    (right_id, left_id)
                };
                self.substitutions.insert(from, Type::Var(to));
            }
            (Type::Var(id), other) | (other, Type::Var(id)) => {
                self.substitutions.insert(id, other);
            }
            (Type::Unknown, _) | (_, Type::Unknown) => {}
            (Type::Number, Type::Number)
            | (Type::Number, Type::NumberWithDimension(_))
            | (Type::NumberWithDimension(_), Type::Number) => {}
            (Type::NumberWithDimension(ld), Type::NumberWithDimension(rd)) => {
                if ld != rd {
                    errors.push(ConstraintError {
                        message: format!("助数詞次元が一致しません。左辺: {} / 右辺: {}", ld, rd),
                        span,
                    });
                }
            }
            (l, r) => {
                errors.push(ConstraintError {
                    message: format!("数値次元の制約に違反しています。左辺: {l:?} / 右辺: {r:?}"),
                    span,
                });
            }
        }
    }
}
