use std::collections::HashMap;

use crate::common::source::Span;
use crate::sema::types::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub declared_at: Span,
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn define(&mut self, name: impl Into<String>, ty: Type, mutable: bool, declared_at: Span) {
        let name = name.into();
        let info = SymbolInfo {
            name: name.clone(),
            ty,
            mutable,
            declared_at,
        };
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, info);
        }
    }

    pub fn assign(&mut self, name: &str, ty: Type, span: Span) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(info) = scope.get_mut(name) {
                info.ty = ty;
                info.declared_at = span;
                return;
            }
        }
        self.define(name.to_string(), ty, true, span);
    }

    pub fn lookup(&self, name: &str) -> Option<&SymbolInfo> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    pub fn apply_type_substitutions<F>(&mut self, mut resolve: F)
    where
        F: FnMut(&Type) -> Type,
    {
        for scope in &mut self.scopes {
            for info in scope.values_mut() {
                info.ty = resolve(&info.ty);
            }
        }
    }
}
