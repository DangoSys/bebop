//! Common utilities for ELF regression tests
//!
//! This module provides shared functionality for scanning ELF files,
//! running bebop commands, and handling test execution.

use assert_cmd::Command;
use clap::Parser;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

/// CLI arguments for the regression test harness
#[derive(Parser, Debug, Clone)]
#[command(name = "elf-regression")]
#[command(about = "ELF regression test harness for bebop")]
pub struct RegressionArgs {
    /// Root directory to scan for ELF files
    #[arg(long, value_name = "DIR")]
    pub elf_root: Option<PathBuf>,

    /// Filter pattern for test names (substring match)
    #[arg(long, value_name = "PATTERN")]
    pub filter: Option<String>,

    /// Keep temporary directories after test completion
    #[arg(long)]
    pub keep_temp: bool,

    /// Number of parallel jobs (currently unused, for future parallel execution)
    #[arg(long, short = 'j', value_name = "N", default_value = "1")]
    pub jobs: usize,

    /// Verbose output
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// cargo-nextest / libtest: discover tests (`cargo-nextest` invokes `--list --format terse`).
    #[arg(long, hide = true)]
    pub list: bool,

    #[arg(long, hide = true)]
    pub format: Option<String>,

    #[arg(long, hide = true)]
    pub ignored: bool,

    #[arg(long, hide = true)]
    pub exact: bool,

    #[arg(long, hide = true)]
    pub nocapture: bool,

    #[arg(long, hide = true)]
    pub bench: bool,

    #[arg(long, hide = true)]
    pub show_output: bool,

    /// Arguments to pass to libtest-mimic (after --)
    #[arg(trailing_var_arg = true)]
    pub test_args: Vec<String>,
}

impl RegressionArgs {
    /// Get the ELF root directory, using default if not specified
    pub fn elf_root(&self) -> PathBuf {
        self.elf_root.clone().unwrap_or_else(|| {
            PathBuf::from("/home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy")
        })
    }

    /// Build argv fragments for [libtest-mimic](https://docs.rs/libtest-mimic) / libtest protocol.
    pub fn libtest_forward_flags(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.exact {
            out.push("--exact".to_string());
        }
        if self.nocapture {
            out.push("--nocapture".to_string());
        }
        if self.show_output {
            out.push("--show-output".to_string());
        }
        if self.bench {
            out.push("--bench".to_string());
        }
        out
    }
}

/// Implements the stdout protocol required by [cargo-nextest](https://nexte.st/docs/design/custom-test-harnesses/).
pub fn write_nextest_terse_list(
    args: &RegressionArgs,
    trial_name: impl Fn(&ElfTestCase) -> String,
) -> std::io::Result<()> {
    use std::io::Write;

    if args.ignored {
        return Ok(());
    }

    let elf_root = args.elf_root();
    if !elf_root.exists() {
        return Ok(());
    }

    let test_cases = scan_elf_files(&elf_root, None);
    let test_cases = filter_tests(test_cases, args.filter.as_deref());

    let mut stdout = std::io::stdout().lock();
    for tc in test_cases {
        writeln!(stdout, "{}: test", trial_name(&tc))?;
    }
    Ok(())
}

/// Represents a discovered ELF test case
#[derive(Debug, Clone)]
pub struct ElfTestCase {
    /// Full path to the ELF file
    pub path: PathBuf,
    /// Test name (derived from file path)
    pub name: String,
    /// File stem (filename without extension)
    pub stem: String,
}

impl ElfTestCase {
    /// Create a new test case from a path
    pub fn from_path(path: PathBuf) -> Option<Self> {
        let stem = path.file_stem()?.to_string_lossy().to_string();
        let name = Self::generate_name(&path);
        Some(Self {
            path,
            name,
            stem,
        })
    }

    /// Generate a hierarchical test name from the path
    /// e.g., /.../toy/matmul.elf -> toy::matmul
    fn generate_name(path: &Path) -> String {
        // Get the parent directory name
        let parent_name = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Get the file stem
        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        format!("{}:: {}", parent_name, file_stem)
    }
}

/// Scan directory for ELF files recursively
pub fn scan_elf_files(root: &Path, extension: Option<&str>) -> Vec<ElfTestCase> {
    let mut tests = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Check if it's a file
        if !path.is_file() {
            continue;
        }

        // Check extension if specified
        if let Some(ext) = extension {
            if path.extension() != Some(OsStr::new(ext)) {
                continue;
            }
        }

        // Try to create test case
        if let Some(test_case) = ElfTestCase::from_path(path.to_path_buf()) {
            tests.push(test_case);
        }
    }

    // Sort by name for deterministic ordering
    tests.sort_by(|a, b| a.name.cmp(&b.name));
    tests
}

/// Filter test cases by name pattern
pub fn filter_tests(tests: Vec<ElfTestCase>, filter: Option<&str>) -> Vec<ElfTestCase> {
    let tests: Vec<_> = match filter {
        Some(pattern) => tests
            .into_iter()
            .filter(|t| t.name.contains(pattern) || t.stem.contains(pattern))
            .collect(),
        None => tests,
    };

    // 默认只保留 singlecore-baremetal 的 ELF
    tests
        .into_iter()
        .filter(|t| t.stem.ends_with("singlecore-baremetal"))
        .collect()
}

/// Result of running a single ELF test
#[derive(Debug)]
pub struct TestResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub temp_dir: Option<PathBuf>,
}

/// Run a single ELF test using bebop binary
pub fn run_elf_test(
    bebop_bin: &Path,
    elf_path: &Path,
    keep_temp: bool,
    verbose: bool,
) -> TestResult {
    // Create temporary directory for this test
    let temp_dir = tempfile::tempdir().ok();
    let temp_path = temp_dir.as_ref().map(|d| d.path().to_path_buf());

    if verbose {
        eprintln!("Running test for: {}", elf_path.display());
        if let Some(ref p) = temp_path {
            eprintln!("  Temp dir: {}", p.display());
        }
    }

    // Build the command
    let mut cmd = Command::new(bebop_bin);
    cmd.arg("bemu");
    cmd.arg("--elf").arg(elf_path);

    // Add log directory if temp dir exists
    if let Some(ref p) = temp_path {
        cmd.arg("--log-dir").arg(p);
    }

    // Set timeout (5 minutes default)
    cmd.timeout(Duration::from_secs(300));

    // Run the command
    let output = cmd.output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Write stdout/stderr to temp files for debugging
            if let Some(ref p) = temp_path {
                let _ = fs::write(p.join("stdout.log"), &stdout);
                let _ = fs::write(p.join("stderr.log"), &stderr);
            }

            let success = output.status.success();

            // Clean up temp dir unless keep_temp is set or test failed
            if !keep_temp && success {
                drop(temp_dir);
            }

            TestResult {
                success,
                stdout,
                stderr,
                exit_code: output.status.code(),
                temp_dir: if keep_temp || !success {
                    temp_path
                } else {
                    None
                },
            }
        }
        Err(e) => {
            let stderr = format!("Failed to execute bebop: {}", e);

            // Write error to temp file
            if let Some(ref p) = temp_path {
                let _ = fs::write(p.join("error.log"), &stderr);
            }

            TestResult {
                success: false,
                stdout: String::new(),
                stderr,
                exit_code: None,
                temp_dir: if keep_temp { temp_path } else { None },
            }
        }
    }
}

/// Run a single ELF test using verilator backend
///
/// This function spawns the prebuilt bebop binary with verilator backend:
///   <bebop_bin> verilator --elf <elf> --log-dir <dir> --fst-dir <dir>
pub fn run_verilator_test(
    bebop_bin: &Path,
    elf_path: &Path,
    keep_temp: bool,
    verbose: bool,
) -> TestResult {
    // Create temporary directory for this test
    let temp_dir = tempfile::tempdir().ok();
    let temp_path = temp_dir.as_ref().map(|d| d.path().to_path_buf());

    // Create log and fst subdirectories
    let log_dir = temp_path.as_ref().map(|p| p.join("log"));
    let fst_dir = temp_path.as_ref().map(|p| p.join("fst"));

    if let Some(ref log) = log_dir {
        let _ = fs::create_dir_all(log);
    }
    if let Some(ref fst) = fst_dir {
        let _ = fs::create_dir_all(fst);
    }

    if verbose {
        eprintln!("Running verilator test for: {}", elf_path.display());
        if let Some(ref p) = temp_path {
            eprintln!("  Temp dir: {}", p.display());
        }
    }

    // Build the command using the already-built bebop binary.
    // Rebuilding through `cargo run` for every test is slow and brittle.
    let mut cmd = Command::new(bebop_bin);
    cmd.arg("verilator");
    cmd.arg("--elf").arg(elf_path);

    // Add log and fst directories
    if let Some(ref log) = log_dir {
        cmd.arg("--log-dir").arg(log);
    }
    if let Some(ref fst) = fst_dir {
        cmd.arg("--fst-dir").arg(fst);
    }

    // Must be >= nextest `slow-timeout` for verilator (see `.config/nextest.toml`).
    cmd.timeout(Duration::from_secs(600));

    // Run the command
    let output = cmd.output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Write stdout/stderr to temp files for debugging
            if let Some(ref p) = temp_path {
                let _ = fs::write(p.join("stdout.log"), &stdout);
                let _ = fs::write(p.join("stderr.log"), &stderr);
            }

            let success = output.status.success();

            // Clean up temp dir unless keep_temp is set or test failed
            if !keep_temp && success {
                drop(temp_dir);
            }

            TestResult {
                success,
                stdout,
                stderr,
                exit_code: output.status.code(),
                temp_dir: if keep_temp || !success {
                    temp_path
                } else {
                    None
                },
            }
        }
        Err(e) => {
            let stderr = format!("Failed to execute verilator: {}", e);

            // Write error to temp file
            if let Some(ref p) = temp_path {
                let _ = fs::write(p.join("error.log"), &stderr);
            }

            TestResult {
                success: false,
                stdout: String::new(),
                stderr,
                exit_code: None,
                temp_dir: if keep_temp { temp_path } else { None },
            }
        }
    }
}

/// Print test failure details
pub fn print_failure_details(result: &TestResult, test_name: &str) {
    eprintln!("\n=== Test Failed: {} ===", test_name);

    if !result.stdout.is_empty() {
        eprintln!("\n--- stdout ---");
        eprintln!("{}", result.stdout);
    }

    if !result.stderr.is_empty() {
        eprintln!("\n--- stderr ---");
        eprintln!("{}", result.stderr);
    }

    if let Some(code) = result.exit_code {
        eprintln!("\nExit code: {}", code);
    }

    if let Some(ref dir) = result.temp_dir {
        eprintln!("\nTemp directory preserved at: {}", dir.display());
    }

    eprintln!("==================\n");
}
