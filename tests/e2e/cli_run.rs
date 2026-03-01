use std::fs;
use std::process::Command;

#[test]
fn cli_run_ok() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let path = "target/tmp_cli_run.kb";
    fs::write(path, "「こんにちは」と 表示する").expect("write file");

    let out = Command::new(bin)
        .arg("run")
        .arg(path)
        .output()
        .expect("run cli");

    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
