use std::fs;
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

    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_test_failure_detail() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let manifest_path = "target/tmp_cli_test_manifest_fail.yaml";

    fs::write(
        manifest_path,
        r#"cases:
  - id: FAIL-CHECK-ACCEPT
    mode: check
    expect: accept
    input: "これ"
"#,
    )
    .expect("write manifest");

    let out = Command::new(bin)
        .arg("test")
        .env("KOTOBA_TEST_MANIFEST", manifest_path)
        .output()
        .expect("run cli test");

    assert!(
        !out.status.success(),
        "expected failure but succeeded: stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stdout.contains("FAIL FAIL-CHECK-ACCEPT"), "stdout={stdout}");
    assert!(
        stderr.contains("check で受理期待だったが、静的検証で拒否された"),
        "stderr={stderr}"
    );
}

#[test]
fn cli_test_rejects_manifest_placeholder_input() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let manifest_path = "target/tmp_cli_test_manifest_placeholder.yaml";

    fs::write(
        manifest_path,
        r#"cases:
  - id: BAD-PLACEHOLDER
    mode: check
    expect: reject
    input: "@"
"#,
    )
    .expect("write manifest");

    let out = Command::new(bin)
        .arg("test")
        .env("KOTOBA_TEST_MANIFEST", manifest_path)
        .output()
        .expect("run cli test");

    assert!(
        !out.status.success(),
        "expected failure but succeeded: stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("manifest 検証失敗"), "stderr={stderr}");
    assert!(stderr.contains("プレースホルダ入力"), "stderr={stderr}");
}

#[test]
fn cli_check_accepts_struct_definition_without_codegen() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let source_path = "target/tmp_cli_check_struct.kb";
    fs::write(source_path, "人 という 組\n  名前は 文字列").expect("write source");

    let out = Command::new(bin)
        .arg("check")
        .arg(source_path)
        .output()
        .expect("run cli check");

    assert!(
        out.status.success(),
        "stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
