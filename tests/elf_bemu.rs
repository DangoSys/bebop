//! BEMU ELF Regression Tests
//!
//! This integration test discovers ELF files and runs them using the bebop bemu backend.
//!
//! Usage:
//!   cargo test --test elf_bemu --features bemu -- --help
//!   cargo test --test elf_bemu --features bemu
//!   cargo test --test elf_bemu --features bemu -- --filter matmul
//!   cargo nextest run --test elf_bemu --features bemu
//!
//! Note: This test requires the 'bemu' feature to be enabled.

use clap::Parser;
use libtest_mimic::{Arguments, Failed, Trial};
use std::path::PathBuf;

mod common;

use common::{
    filter_tests, print_failure_details, run_elf_test, scan_elf_files, write_nextest_terse_list,
    RegressionArgs,
};

fn main() -> std::process::ExitCode {
    // Parse our custom arguments
    let args = RegressionArgs::parse();

    if args.list {
        if args.format.as_deref() != Some("terse") {
            eprintln!("error: --list requires --format terse");
            return std::process::ExitCode::FAILURE;
        }
        if let Err(e) =
            write_nextest_terse_list(&args, |tc| format!("bemu:: {}", tc.name))
        {
            eprintln!("error: {e}");
            return std::process::ExitCode::FAILURE;
        }
        return std::process::ExitCode::SUCCESS;
    }

    // Get the bebop binary path (set by cargo during compilation)
    let bebop_bin = PathBuf::from(env!("CARGO_BIN_EXE_bebop"));

    if !bebop_bin.exists() {
        eprintln!("Error: bebop binary not found at: {}", bebop_bin.display());
        eprintln!("Make sure to build with: cargo build --features bemu");
        return std::process::ExitCode::FAILURE;
    }

    // Get ELF root directory
    let elf_root = args.elf_root();

    if !elf_root.exists() {
        eprintln!("Error: ELF root directory not found: {}", elf_root.display());
        return std::process::ExitCode::FAILURE;
    }

    // Scan for ELF files
    let test_cases = scan_elf_files(&elf_root, None);

    if test_cases.is_empty() {
        eprintln!("Warning: No ELF files found in: {}", elf_root.display());
        return std::process::ExitCode::SUCCESS;
    }

    if args.verbose {
        eprintln!("Found {} ELF test cases", test_cases.len());
        for tc in &test_cases {
            eprintln!("  - {}", tc.name);
        }
    }

    // Filter tests if requested
    let test_cases = filter_tests(test_cases, args.filter.as_deref());

    if args.verbose {
        eprintln!("Running {} tests after filtering", test_cases.len());
    }

    // Convert test cases to libtest-mimic Trials
    let trials: Vec<Trial> = test_cases
        .into_iter()
        .map(|test_case| {
            let name = format!("bemu:: {}", test_case.name);
            let bebop_bin = bebop_bin.clone();
            let elf_path = test_case.path.clone();
            let keep_temp = args.keep_temp;
            let verbose = args.verbose;

            Trial::test(name, move || {
                run_single_test(&bebop_bin, &elf_path, keep_temp, verbose)
            })
        })
        .collect();

    // Parse test arguments for libtest-mimic
    let mut libtest_args = vec!["elf_bemu".to_string()];
    libtest_args.extend(args.libtest_forward_flags());
    libtest_args.extend(args.test_args);
    let test_args = Arguments::from_iter(libtest_args);

    // Run the tests
    let conclusion = libtest_mimic::run(&test_args, trials);

    // Exit with appropriate code
    conclusion.exit_code()
}

/// Run a single ELF test
fn run_single_test(
    bebop_bin: &PathBuf,
    elf_path: &PathBuf,
    keep_temp: bool,
    verbose: bool,
) -> Result<(), Failed> {
    let result = run_elf_test(bebop_bin, elf_path, keep_temp, verbose);

    if result.success {
        Ok(())
    } else {
        // Print failure details to stderr
        let test_name = elf_path.file_stem().unwrap_or_default().to_string_lossy();
        print_failure_details(&result, &test_name);

        // Return failure
        Err(format!("Test failed: {}", elf_path.display()).into())
    }
}
