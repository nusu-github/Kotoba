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
