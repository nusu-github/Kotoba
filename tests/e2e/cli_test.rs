use std::process::Command;

#[test]
fn cli_test_manifest() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let out = Command::new(bin)
        .arg("test")
        .arg("--filter")
        .arg("RUN-ACCEPT-001")
        .output()
        .expect("run cli test");

    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
}
