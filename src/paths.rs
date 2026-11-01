use std::path::{Path, PathBuf};

pub fn buckyball(bebop: &Path) -> PathBuf {
    let bebop = bebop.canonicalize().expect("Bebop source directory");
    if bebop.parent().unwrap().join("bbdev").is_dir() {
        bebop.parent().unwrap().to_path_buf()
    } else {
        bebop.join("thirdparty/buckyball")
    }
}
