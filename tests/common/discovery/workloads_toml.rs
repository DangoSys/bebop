use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WorkloadsConfig {
    workloads: Workloads,
}

#[derive(Debug, Deserialize)]
struct Workloads {
    search_path: Option<String>,
    tests: Vec<String>,
}

#[derive(Debug)]
pub struct WorkloadSpec {
    pub search_path: Option<PathBuf>,
    pub tests: Vec<String>,
}

pub fn load_workload_spec(toml_path: &Path) -> Result<WorkloadSpec, WorkloadTomlError> {
    let content = fs::read_to_string(toml_path).map_err(|source| WorkloadTomlError::Read {
        path: toml_path.to_path_buf(),
        source,
    })?;
    let config: WorkloadsConfig =
        toml::from_str(&content).map_err(|source| WorkloadTomlError::Parse {
            path: toml_path.to_path_buf(),
            source,
        })?;
    if config.workloads.tests.is_empty() {
        return Err(WorkloadTomlError::Empty {
            path: toml_path.to_path_buf(),
        });
    }
    Ok(WorkloadSpec {
        search_path: config.workloads.search_path.map(PathBuf::from),
        tests: config.workloads.tests,
    })
}

#[derive(Debug)]
pub enum WorkloadTomlError {
    Read { path: PathBuf, source: std::io::Error },
    Parse { path: PathBuf, source: toml::de::Error },
    Empty { path: PathBuf },
}

impl std::fmt::Display for WorkloadTomlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkloadTomlError::Read { path, source } => {
                write!(f, "Failed to read workload TOML {}: {}", path.display(), source)
            }
            WorkloadTomlError::Parse { path, source } => {
                write!(f, "Failed to parse workload TOML {}: {}", path.display(), source)
            }
            WorkloadTomlError::Empty { path } => {
                write!(f, "Workload TOML {} has no tests under [workloads]", path.display())
            }
        }
    }
}

impl std::error::Error for WorkloadTomlError {}
