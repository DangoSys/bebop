use assert_cmd::Command;
use clap::Parser;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use walkdir::WalkDir;

pub mod artifact;
pub mod backend;
pub mod result;

pub use artifact::ArtifactManager;
pub use backend::BackendRunner;
#[cfg(feature = "bemu")]
pub use backend::BemuBackend;
#[cfg(feature = "verilator")]
pub use backend::VerilatorBackend;
pub use result::{RegressionResult, TestStatus};

#[derive(Parser, Debug, Clone)]
#[command(name = "elf-regression")]
#[command(about = "ELF regression test harness for bebop")]
pub struct RegressionArgs {
    #[arg(long, value_name = "DIR")]
    pub elf_root: Option<PathBuf>,

    #[arg(long, value_name = "PATTERN")]
    pub filter: Option<String>,

    #[arg(long)]
    pub keep_temp: bool,

    #[arg(long, short = 'j', value_name = "N", default_value = "1")]
    pub jobs: usize,

    #[arg(long, short = 'v')]
    pub verbose: bool,

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

    #[arg(trailing_var_arg = true)]
    pub test_args: Vec<String>,
}

impl RegressionArgs {
    pub fn elf_root(&self) -> PathBuf {
        self.elf_root.clone().unwrap_or_else(|| {
            PathBuf::from("../bb-tests/output/workloads/src/CTest/toy")
        })
    }

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
        Some(Self {
            path,
            name,
            stem,
        })
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

pub fn scan_elf_files(root: &Path, extension: Option<&str>) -> Vec<ElfTestCase> {
    let mut tests = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if let Some(ext) = extension {
            if path.extension() != Some(OsStr::new(ext)) {
                continue;
            }
        }

        if let Some(test_case) = ElfTestCase::from_path(path.to_path_buf()) {
            tests.push(test_case);
        }
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));
    tests
}

pub fn filter_tests(tests: Vec<ElfTestCase>, filter: Option<&str>) -> Vec<ElfTestCase> {
    let tests: Vec<_> = match filter {
        Some(pattern) => tests
            .into_iter()
            .filter(|t| t.name.contains(pattern) || t.stem.contains(pattern))
            .collect(),
        None => tests,
    };

    tests
        .into_iter()
        .filter(|t| t.stem.ends_with("singlecore-baremetal"))
        .collect()
}

pub fn run_elf_regression<B, F>(
    args: RegressionArgs,
    harness_binary_name: &'static str,
    test_case_display_name: F,
    missing_bebop_hint: &'static str,
    backend: B,
) -> ExitCode
where
    B: BackendRunner + Clone + Send + Sync + 'static,
    F: Fn(&ElfTestCase) -> String + Clone + Send + Sync + 'static,
{
    use libtest_mimic::{Arguments, Trial};

    if args.list {
        if args.format.as_deref() != Some("terse") {
            eprintln!("error: --list requires --format terse");
            return ExitCode::FAILURE;
        }
        let name_fn = test_case_display_name.clone();
        if let Err(e) = write_nextest_terse_list(&args, move |tc| name_fn(tc)) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    let bebop_bin = PathBuf::from(env!("CARGO_BIN_EXE_bebop"));

    if !bebop_bin.exists() {
        eprintln!("Error: bebop binary not found at: {}", bebop_bin.display());
        eprintln!("{missing_bebop_hint}");
        return ExitCode::FAILURE;
    }

    let elf_root = args.elf_root();

    if !elf_root.exists() {
        eprintln!("Error: ELF root directory not found: {}", elf_root.display());
        return ExitCode::FAILURE;
    }

    let test_cases = scan_elf_files(&elf_root, None);

    if test_cases.is_empty() {
        eprintln!("Warning: No ELF files found in: {}", elf_root.display());
        return ExitCode::SUCCESS;
    }

    if args.verbose {
        eprintln!("Found {} ELF test cases", test_cases.len());
        for tc in &test_cases {
            eprintln!("  - {}", tc.name);
        }
    }

    let test_cases = filter_tests(test_cases, args.filter.as_deref());

    if args.verbose {
        eprintln!("Running {} tests after filtering", test_cases.len());
    }

    let trials: Vec<Trial> = test_cases
        .into_iter()
        .map(|test_case| {
            let name = test_case_display_name.clone()(&test_case);
            let bebop_bin = bebop_bin.clone();
            let elf_path = test_case.path.clone();
            let keep_temp = args.keep_temp;
            let verbose = args.verbose;
            let backend = backend.clone();

            Trial::test(name, move || {
                let result = run_backend_elf_test(
                    &bebop_bin,
                    &elf_path,
                    keep_temp,
                    verbose,
                    &backend,
                );
                if result.success() {
                    Ok(())
                } else {
                    let test_name =
                        elf_path.file_stem().unwrap_or_default().to_string_lossy();
                    print_failure_details(&result, &test_name);
                    Err(format!("Test failed: {}", elf_path.display()).into())
                }
            })
        })
        .collect();

    let mut libtest_args = vec![harness_binary_name.to_string()];
    libtest_args.extend(args.libtest_forward_flags());
    libtest_args.extend(args.test_args);
    let test_args = Arguments::from_iter(libtest_args);

    libtest_mimic::run(&test_args, trials).exit_code()
}

pub fn run_backend_elf_test(
    bebop_bin: &Path,
    elf_path: &Path,
    keep_temp: bool,
    verbose: bool,
    backend: &impl BackendRunner,
) -> RegressionResult {
    let workload_name = elf_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let artifacts = match ArtifactManager::create_named(&workload_name, keep_temp) {
        Ok(a) => a,
        Err(e) => {
            return RegressionResult::infra_error(
                backend.backend_name(),
                &workload_name,
                elf_path,
                None,
                format!("Failed to create artifact directory: {}", e),
            );
        }
    };

    if verbose {
        eprintln!(
            "Running {} for: {}",
            backend.verbose_run_kind(),
            elf_path.display()
        );
        eprintln!("  Artifact dir: {}", artifacts.root().display());
    }

    let mut cmd = Command::new(bebop_bin);
    backend.configure_command_env(&mut cmd);
    backend.build_command(&mut cmd, bebop_bin, elf_path, &artifacts);
    cmd.timeout(backend.timeout());

    let start = Instant::now();
    let output = cmd.output();
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as u64;

    let stdout_path = artifacts.stdout_path();
    let stderr_path = artifacts.stderr_path();
    let fst_path = if backend.needs_fst_dir() {
        Some(artifacts.fst_waveform_path())
    } else {
        None
    };
    let log_path = if backend.needs_log_dir() {
        Some(artifacts.log_dir())
    } else {
        None
    };

    let result = match output {
        Ok(output) => {
            let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

            let _ = artifacts.write_stdout(&stdout_str);
            let _ = artifacts.write_stderr(&stderr_str);

            let status = if output.status.success() {
                TestStatus::Pass
            } else if output.status.code().is_none() {
                TestStatus::Crash
            } else {
                TestStatus::Fail
            };

            RegressionResult {
                backend: backend.backend_name().to_string(),
                workload_name,
                elf_path: elf_path.to_path_buf(),
                elapsed_ms,
                status,
                exit_code: output.status.code(),
                stdout_path: Some(stdout_path),
                stderr_path: Some(stderr_path),
                artifact_dir: Some(artifacts.root().to_path_buf()),
                fst_path,
                log_path,
                error_message: None,
            }
        }
        Err(e) => {
            let status = if e.kind() == std::io::ErrorKind::TimedOut {
                TestStatus::Timeout
            } else {
                TestStatus::InfraError
            };

            let error_message = format!("{}", e);

            RegressionResult {
                backend: backend.backend_name().to_string(),
                workload_name,
                elf_path: elf_path.to_path_buf(),
                elapsed_ms,
                status,
                exit_code: None,
                stdout_path: Some(stdout_path),
                stderr_path: Some(stderr_path),
                artifact_dir: Some(artifacts.root().to_path_buf()),
                fst_path,
                log_path,
                error_message: Some(error_message),
            }
        }
    };

    let _ = artifacts.write_summary(&result);

    let _preserved = artifacts.finalize(result.success());

    result
}

pub fn print_failure_details(result: &RegressionResult, test_name: &str) {
    eprintln!("\n=== Test Failed: {} ===", test_name);
    eprintln!("Status: {}", result.status);
    eprintln!("Backend: {}", result.backend);
    eprintln!("Elapsed: {}ms", result.elapsed_ms);

    if let Some(ref msg) = result.error_message {
        eprintln!("Error: {}", msg);
    }

    if let Some(code) = result.exit_code {
        eprintln!("Exit code: {}", code);
    }

    if let Some(ref dir) = result.artifact_dir {
        eprintln!("Artifact dir: {}", dir.display());
    }

    if let Some(ref p) = result.stderr_path {
        if p.exists() {
            if let Ok(content) = std::fs::read_to_string(p) {
                if !content.is_empty() {
                    eprintln!("\n--- stderr ---");
                    eprintln!("{}", content);
                }
            }
        }
    }

    if let Some(ref p) = result.stdout_path {
        if p.exists() {
            if let Ok(content) = std::fs::read_to_string(p) {
                if !content.is_empty() {
                    eprintln!("\n--- stdout ---");
                    eprintln!("{}", content);
                }
            }
        }
    }

    eprintln!("==================\n");
}
