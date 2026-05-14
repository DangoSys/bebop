use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Pass,
    Fail,
    Timeout,
    Crash,
    InfraError,
}

impl TestStatus {
    pub fn is_success(self) -> bool {
        self == TestStatus::Pass
    }
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TestStatus::Pass => "pass",
            TestStatus::Fail => "fail",
            TestStatus::Timeout => "timeout",
            TestStatus::Crash => "crash",
            TestStatus::InfraError => "infra_error",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionResult {
    pub backend: String,
    pub workload_name: String,
    pub elf_path: PathBuf,
    pub elapsed_ms: u64,
    pub status: TestStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fst_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl RegressionResult {
    pub fn success(&self) -> bool {
        self.status.is_success()
    }

    pub fn write_summary(&self, dir: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(dir.join("summary.json"), json)
    }

    pub fn infra_error(
        backend: &str,
        workload_name: &str,
        elf_path: &Path,
        artifact_dir: Option<PathBuf>,
        error_message: String,
    ) -> Self {
        RegressionResult {
            backend: backend.to_string(),
            workload_name: workload_name.to_string(),
            elf_path: elf_path.to_path_buf(),
            elapsed_ms: 0,
            status: TestStatus::InfraError,
            exit_code: None,
            stdout_path: None,
            stderr_path: None,
            artifact_dir,
            fst_path: None,
            log_path: None,
            error_message: Some(error_message),
        }
    }
}
