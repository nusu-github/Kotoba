use std::fs;
use std::path::{Path, PathBuf};

use crate::common::source::SourceFile;

pub fn load_source(path: &Path) -> std::io::Result<SourceFile> {
    let content = fs::read_to_string(path)?;
    Ok(SourceFile::new(path.display().to_string(), content))
}

pub fn normalize_module_path(base_file: &Path, module_name: &str) -> PathBuf {
    let mut p = base_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(module_name);
    if p.extension().is_none() {
        p.set_extension("kb");
    }
    p
}
