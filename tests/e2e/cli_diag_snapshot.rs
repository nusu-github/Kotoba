use std::fs;
use std::process::Command;

use insta::assert_snapshot;

#[test]
fn cli_check_diagnostic_snapshot() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let path = "target/tmp_cli_diag_snapshot.kb";
    fs::write(path, "名前 「太郎」").expect("write file");

    let out = Command::new(bin)
        .arg("check")
        .arg(path)
        .output()
        .expect("run cli");

    assert!(
        !out.status.success(),
        "expected failure but succeeded: stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = String::from_utf8_lossy(&out.stderr)
        .replace(path, "<INPUT>")
        .replace("\r\n", "\n");

    insta::with_settings!({prepend_module_to_snapshot => false}, {
        assert_snapshot!("cli_check_diagnostic", stderr);
    });
}
