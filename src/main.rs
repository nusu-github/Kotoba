mod ast;
mod bytecode;
mod compiler;
mod lexer;
mod parser;
mod source;
mod token;
mod vm;

use std::env;
use std::fs;
use std::process;

use compiler::Compiler;
use lexer::Lexer;
use parser::Parser;
use source::SourceFile;
use vm::VM;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("使い方: kotoba <ファイル.kb>");
        eprintln!("  例: kotoba hello.kb");
        process::exit(1);
    }

    let filename = &args[1];
    let source_code = match fs::read_to_string(filename) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("ファイル「{}」を読み込めません: {}", filename, e);
            process::exit(1);
        }
    };

    let source_file = SourceFile::new(filename, &source_code);

    // 字句解析
    let (tokens, lex_errors) = Lexer::new(&source_code).tokenize();
    if !lex_errors.is_empty() {
        for err in &lex_errors {
            let loc = source_file.line_col(err.span.start);
            eprintln!(
                "{}:{}:{}: 字句エラー: {}",
                filename, loc.line, loc.column, err.message
            );
        }
        process::exit(1);
    }

    // 構文解析
    let (program, parse_errors) = Parser::new(tokens).parse();
    if !parse_errors.is_empty() {
        for err in &parse_errors {
            let loc = source_file.line_col(err.span.start);
            eprintln!(
                "{}:{}:{}: 構文エラー: {}",
                filename, loc.line, loc.column, err.message
            );
        }
        process::exit(1);
    }

    // コンパイル
    let chunks = match Compiler::new().compile(&program) {
        Ok(chunks) => chunks,
        Err(errors) => {
            for err in &errors {
                eprintln!("{}: コンパイルエラー: {}", filename, err.message);
            }
            process::exit(1);
        }
    };

    // デバッグフラグ: --debug でバイトコードをダンプ
    if args.iter().any(|a| a == "--debug") {
        for chunk in &chunks {
            eprintln!("{}", chunk.disassemble());
        }
    }

    // 実行
    let mut vm_instance = VM::new(chunks);
    match vm_instance.run() {
        Ok(_) => {}
        Err(err) => {
            eprintln!("{}: {}", filename, err);
            process::exit(1);
        }
    }
}
