use std::process::Command;

#[test]
fn conformance_smoke() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let out = Command::new(bin)
        .arg("test")
        .output()
        .expect("run conformance");

    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
}
