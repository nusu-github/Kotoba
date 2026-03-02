use crate::backend::value::{Chunk, OpCode, Value};

#[derive(Debug, Clone)]
pub struct RirProgram {
    pub functions: Vec<RirFunction>,
}

#[derive(Debug, Clone)]
pub struct RirFunction {
    pub name: String,
    pub arity: usize,
    pub local_count: usize,
    pub constants: Vec<Value>,
    pub body: Vec<RirInst>,
}

#[derive(Debug, Clone)]
pub enum RirInst {
    Op(OpCode),
}

#[derive(Debug, Clone)]
pub struct RegProgram {
    chunks: Vec<Chunk>,
}

impl RirProgram {
    pub fn from_chunks(chunks: &[Chunk]) -> Self {
        let functions = chunks
            .iter()
            .map(|chunk| RirFunction {
                name: chunk.name.clone(),
                arity: chunk.arity,
                local_count: chunk.local_count,
                constants: chunk.constants.clone(),
                body: chunk.code.iter().cloned().map(RirInst::Op).collect(),
            })
            .collect();
        Self { functions }
    }

    pub fn into_reg_program(self) -> RegProgram {
        let chunks = self
            .functions
            .into_iter()
            .map(|func| Chunk {
                name: func.name,
                code: func
                    .body
                    .into_iter()
                    .map(|inst| match inst {
                        RirInst::Op(op) => op,
                    })
                    .collect(),
                constants: func.constants,
                arity: func.arity,
                local_count: func.local_count,
            })
            .collect();
        RegProgram { chunks }
    }
}

impl RegProgram {
    pub fn from_chunks(chunks: Vec<Chunk>) -> Self {
        Self { chunks }
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    pub fn into_chunks(self) -> Vec<Chunk> {
        self.chunks
    }
}
