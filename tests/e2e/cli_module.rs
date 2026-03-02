use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = format!("target/tmp_cli_module_{ts}");
    fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn cli_run_with_use_module() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let dir = temp_dir();
    let main_path = format!("{dir}/main.kb");
    let util_path = format!("{dir}/util.kb");

    fs::write(
        &util_path,
        "公開 足す という 手順【a:を、b:に】\n  aとbの和を 返す\n",
    )
    .expect("write util");
    fs::write(
        &main_path,
        "「util」を 使う\n結果 は 2を 3に 足す\n結果と 表示する\n",
    )
    .expect("write main");

    let out = Command::new(bin)
        .arg("run")
        .arg(Path::new(&main_path))
        .output()
        .expect("run cli");

    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains('5'));
}

#[test]
fn cli_check_reports_import_module_span() {
    let bin = env!("CARGO_BIN_EXE_kotoba");
    let dir = temp_dir();
    let main_path = format!("{dir}/main.kb");
    let module_name = "bad_mod";
    let mod_path = format!("{dir}/{module_name}.kb");

    fs::write(
        &mod_path,
        "公開 飾る という 手順【本文:を】\n  前半 は 「<<」と本文の和\n  前半と「>>」の和を 返す\n",
    )
    .expect("write module");
    fs::write(&main_path, format!("「{module_name}」を 使う\n")).expect("write main");

    let out = Command::new(bin)
        .arg("check")
        .arg(Path::new(&main_path))
        .output()
        .expect("run cli");

    assert!(
        !out.status.success(),
        "unexpected success: stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bad_mod.kb"),
        "stderr missing module path: {stderr}"
    );
    assert!(
        !stderr.contains("OutOfBounds"),
        "stderr contains out-of-bounds: {stderr}"
    );
    assert!(
        stderr.contains("前半と「>>」の和を 返す"),
        "stderr missing offending line: {stderr}"
    );
}
