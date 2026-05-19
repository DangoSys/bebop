use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub fn collect_files(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_inner(root, exts, &mut files);
    files.sort();
    files
}

fn collect_files_inner(root: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root).unwrap_or_else(|_| panic!("read {}", root.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_inner(&path, exts, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };
        if exts.iter().any(|candidate| candidate.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
}

pub fn write_flist(path: &Path, sources: &[PathBuf]) {
    let mut contents = String::new();
    for src in sources {
        contents.push_str(&src.display().to_string());
        contents.push('\n');
    }
    fs::write(path, contents).expect("write p2e vvac filelist");
}

pub fn clean_build_artifacts(out_dir: &Path) {
    if !out_dir.exists() {
        return;
    }

    let vvac_dir = out_dir.join("vvacDir");
    if vvac_dir.exists() {
        fs::remove_dir_all(&vvac_dir).ok();
        println!("cargo:warning=Removed old vvacDir");
    }

    if let Ok(entries) = fs::read_dir(out_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(OsStr::to_str) {
                if ext == "vm" || ext == "log" {
                    fs::remove_file(&path).ok();
                }
            }
        }
    }
}

pub fn assert_exists(path: &Path, message: &str) {
    assert!(path.exists(), "{message}: {}", path.display());
}
