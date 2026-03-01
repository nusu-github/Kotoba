use std::fs;
use std::process::Command;

#[test]
fn cli_check_ok() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let path = "target/tmp_cli_check.kb";
    fs::write(path, "名前は「太郎」").expect("write file");

    let out = Command::new(bin)
        .arg("check")
        .arg(path)
        .output()
        .expect("run cli");

    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
}
