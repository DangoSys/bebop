use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WorkloadsConfig {
    workloads: Workloads,
}

#[derive(Debug, Deserialize)]
struct Workloads {
    tests: Vec<String>,
}

#[derive(Debug)]
pub struct WorkloadSpec {
    pub tests: Vec<String>,
}

pub fn load_workload_spec(toml_path: &Path) -> Result<WorkloadSpec, WorkloadTomlError> {
    let content = fs::read_to_string(toml_path).map_err(|source| WorkloadTomlError::Read {
        path: toml_path.to_path_buf(),
        source,
    })?;
    let config: WorkloadsConfig = toml::from_str(&content).map_err(|source| WorkloadTomlError::Parse {
        path: toml_path.to_path_buf(),
        source,
    })?;
    if config.workloads.tests.is_empty() {
        return Err(WorkloadTomlError::Empty {
            path: toml_path.to_path_buf(),
        });
    }
    let mut seen = BTreeSet::new();
    let mut tests = Vec::with_capacity(config.workloads.tests.len());
    for test in config.workloads.tests {
        if test.is_empty() || Path::new(&test).file_name().and_then(|name| name.to_str()) != Some(test.as_str()) {
            return Err(WorkloadTomlError::InvalidName {
                path: toml_path.to_path_buf(),
                workload: test,
            });
        }
        if !seen.insert(test.clone()) {
            return Err(WorkloadTomlError::Duplicate {
                path: toml_path.to_path_buf(),
                workload: test,
            });
        }
        tests.push(test);
    }
    Ok(WorkloadSpec { tests })
}

#[derive(Debug)]
pub enum WorkloadTomlError {
    Read { path: PathBuf, source: std::io::Error },
    Parse { path: PathBuf, source: toml::de::Error },
    Empty { path: PathBuf },
    InvalidName { path: PathBuf, workload: String },
    Duplicate { path: PathBuf, workload: String },
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
            WorkloadTomlError::InvalidName { path, workload } => write!(
                f,
                "Workload TOML {} has invalid workload file name {}",
                path.display(),
                workload
            ),
            WorkloadTomlError::Duplicate { path, workload } => write!(
                f,
                "Workload TOML {} repeats workload file name {}",
                path.display(),
                workload
            ),
        }
    }
}

impl std::error::Error for WorkloadTomlError {}
