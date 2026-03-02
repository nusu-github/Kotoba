use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

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

#[test]
fn cli_run_reads_input_suru() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let path = "target/tmp_cli_input_run.kb";
    fs::write(path, "名前 は 入力する\n名前と 表示する").expect("write file");

    let mut child = Command::new(bin)
        .arg("run")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cli");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all("太郎\n".as_bytes()).expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait cli");

    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("太郎"), "stdout={stdout}");
}
