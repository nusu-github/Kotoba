pub mod backend;
pub mod common;
pub mod diag;
pub mod frontend;
pub mod module;
pub mod sema;

// Compatibility re-exports for migrated code during v1 cutover.
pub mod ast {
    pub use crate::frontend::ast::*;
}

pub mod token {
    pub use crate::frontend::token::*;
}

pub mod lexer {
    pub use crate::frontend::lexer::*;
}

pub mod parser {
    pub use crate::frontend::parser::*;
}

pub mod source {
    pub use crate::common::source::*;
}

pub mod bytecode {
    pub use crate::backend::value::*;
}

pub mod compiler {
    pub use crate::backend::codegen::*;
}

pub mod vm {
    pub use crate::backend::vm::*;
}
