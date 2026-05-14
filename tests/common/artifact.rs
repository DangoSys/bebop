use std::fs;
use std::path::{Path, PathBuf};

use super::result::RegressionResult;

const DIR_LOG: &str = "log";
const DIR_FST: &str = "fst";
const FILE_STDOUT: &str = "stdout.log";
const FILE_STDERR: &str = "stderr.log";
const FILE_SUMMARY: &str = "summary.json";
const FILE_WAVEFORM: &str = "waveform.fst";

pub struct ArtifactManager {
    root: PathBuf,
    temp_dir: Option<tempfile::TempDir>,
    keep_temp: bool,
}

impl ArtifactManager {
    pub fn create(keep_temp: bool) -> std::io::Result<Self> {
        let temp_dir = tempfile::TempDir::new()?;
        let root = temp_dir.path().to_path_buf();

        fs::create_dir_all(root.join(DIR_LOG))?;
        fs::create_dir_all(root.join(DIR_FST))?;

        Ok(ArtifactManager {
            root,
            temp_dir: Some(temp_dir),
            keep_temp,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn stdout_path(&self) -> PathBuf {
        self.root.join(FILE_STDOUT)
    }

    pub fn stderr_path(&self) -> PathBuf {
        self.root.join(FILE_STDERR)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.root.join(DIR_LOG)
    }

    pub fn fst_dir(&self) -> PathBuf {
        self.root.join(DIR_FST)
    }

    pub fn fst_waveform_path(&self) -> PathBuf {
        self.fst_dir().join(FILE_WAVEFORM)
    }

    pub fn summary_path(&self) -> PathBuf {
        self.root.join(FILE_SUMMARY)
    }

    pub fn write_stdout(&self, content: &str) -> std::io::Result<()> {
        fs::write(self.stdout_path(), content)
    }

    pub fn write_stderr(&self, content: &str) -> std::io::Result<()> {
        fs::write(self.stderr_path(), content)
    }

    pub fn write_summary(&self, result: &RegressionResult) -> std::io::Result<()> {
        result.write_summary(&self.root)
    }

    pub fn finalize(self, test_passed: bool) -> Option<PathBuf> {
        let should_preserve = self.keep_temp || !test_passed;
        if should_preserve {
            if let Some(td) = self.temp_dir {
                let preserved_path = td.keep();
                Some(preserved_path)
            } else {
                Some(self.root.clone())
            }
        } else {
            drop(self.temp_dir);
            None
        }
    }
}

impl std::fmt::Debug for ArtifactManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactManager")
            .field("root", &self.root)
            .field("keep_temp", &self.keep_temp)
            .finish()
    }
}
