use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use kotoba::backend::codegen::Compiler;
use kotoba::backend::rir::RirProgram;
use kotoba::backend::vm::RegVM;
use kotoba::module::resolver::resolve_root_program;

fn temp_dir(prefix: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kotoba_{prefix}_{ts}"));
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn module_resolver_imports_public_symbol() {
    let dir = temp_dir("import");
    let mod_path = dir.join("math.kb");
    let root_path = dir.join("main.kb");

    fs::write(
        &mod_path,
        "公開 足す という 手順【a:を、b:に】\n  aとbの和を 返す\n",
    )
    .expect("write module");

    fs::write(
        &root_path,
        "「math」を 使う\n結果 は 1を 2に 足す\n結果と 表示する\n",
    )
    .expect("write root");

    let resolved = resolve_root_program(&root_path).expect("resolve modules");
    let chunks = Compiler::new()
        .compile(&resolved.program)
        .expect("compile resolved program");
    let rir = RirProgram::from_chunks(&chunks);
    let mut vm = RegVM::new(rir.into_reg_program());
    vm.run().expect("run");
    assert_eq!(vm.output(), &["3".to_string()]);
}

#[test]
fn module_resolver_detects_cycle() {
    let dir = temp_dir("cycle");
    let a_path = dir.join("a.kb");
    let b_path = dir.join("b.kb");

    fs::write(&a_path, "「b」を 使う\n").expect("write a");
    fs::write(&b_path, "「a」を 使う\n").expect("write b");

    let err = resolve_root_program(&a_path).expect_err("cycle must fail");
    let msg = err
        .iter()
        .map(|d| d.diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(msg.contains("循環"), "msg={msg}");
}
