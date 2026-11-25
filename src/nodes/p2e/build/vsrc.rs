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

pub fn write_flist(path: &Path, rtl_dir: &Path, sources: &[PathBuf], diff: bool, itrace: bool, mtrace: bool) {
    // Headers emitted next to the RTL (e.g. common_cells/*.svh) resolve from its root.
    let mut contents = format!("+incdir+{}\n", rtl_dir.display());
    contents.push_str("+define+BUCKYBALL_DISABLE_PMCTRACE_DPI\n");
    if !diff && !itrace && !mtrace {
        contents.push_str("+define+BUCKYBALL_DISABLE_TRACE_DPI\n");
    }
    if !itrace {
        contents.push_str("+define+BUCKYBALL_DISABLE_ITRACE_DPI\n");
    }
    if !mtrace {
        contents.push_str("+define+BUCKYBALL_DISABLE_MTRACE_DPI\n");
    }
    if !diff {
        contents.push_str("+define+BUCKYBALL_DISABLE_BTRACE_DPI\n");
    }
    for src in sources {
        contents.push_str(&src.display().to_string());
        contents.push('\n');
    }
    fs::write(path, contents).expect("write p2e vvac filelist");
}

pub fn preserve_native_sources(out_dir: &Path, sources: &mut [PathBuf]) -> Vec<String> {
    let mut native = Vec::new();
    let stub_dir = out_dir.join("native/stubs");
    let rtl_dir = out_dir.join("native/rtl");
    fs::create_dir_all(&stub_dir).expect("create native stub directory");
    fs::create_dir_all(&rtl_dir).expect("create native RTL directory");
    for source in sources {
        let text = fs::read_to_string(&source).expect("read RTL source");
        let Some(marker) = text.find("buckyball_p2e_native") else {
            continue;
        };
        assert!(
            !text.contains("DPI-C"),
            "native P2E RTL must not contain DPI callbacks: {}",
            source.display()
        );
        let declaration = text[marker..].find("module ").unwrap() + marker;
        let name_start = declaration + "module ".len();
        let name_end = text[name_start..]
            .find(|character: char| character.is_whitespace() || character == '(' || character == '#')
            .unwrap()
            + name_start;
        let name = text[name_start..name_end].to_string();
        assert!(
            text[name_end..].trim_start().starts_with('('),
            "native P2E entry must have concrete ports: {name}"
        );
        let stub = stub_dir.join(format!("{name}.sv"));
        fs::copy(source.parent().unwrap().join(format!("{name}.stub")), &stub).expect("copy native port declarations");
        let rtl = rtl_dir.join(format!("{name}.sv"));
        fs::copy(source.parent().unwrap().join(format!("{name}.native")), &rtl).expect("snapshot native RTL");
        native.push((name, rtl));
        *source = stub;
    }
    fs::write(
        out_dir.join("native_sources.json"),
        serde_json::to_vec_pretty(&native).unwrap(),
    )
    .expect("write native RTL manifest");
    native.into_iter().map(|(module, _)| module).collect()
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
