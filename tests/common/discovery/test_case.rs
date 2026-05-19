use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ElfTestCase {
    pub path: PathBuf,
    pub name: String,
    pub stem: String,
}

impl ElfTestCase {
    pub fn from_path(path: PathBuf) -> Option<Self> {
        let stem = path.file_stem()?.to_string_lossy().to_string();
        let name = Self::generate_name(&path);
        Some(Self { path, name, stem })
    }

    fn generate_name(path: &Path) -> String {
        let parent_name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        format!("{}::{}", parent_name, file_stem)
    }
}
