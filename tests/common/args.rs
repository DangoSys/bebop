use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "elf-regression")]
#[command(about = "ELF regression test harness for bebop")]
pub struct RegressionArgs {
    #[arg(long, value_name = "PATTERN")]
    pub filter: Option<String>,

    #[arg(long, value_name = "FILE")]
    pub case_list: Option<PathBuf>,

    #[arg(long)]
    pub clean_before: bool,

    #[arg(long)]
    pub diff: bool,

    #[arg(long)]
    pub workload_toml: Option<PathBuf>,

    #[arg(long, default_value = "../bb-tests/output")]
    pub bb_tests_root: PathBuf,

    #[arg(long)]
    pub arch_config: Option<String>,

    #[arg(long)]
    pub rushb_backend: Option<String>,

    #[cfg(feature = "p2e")]
    #[arg(long)]
    pub p2e_bitstream: Option<PathBuf>,

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
    pub fn workload_toml(&self) -> Option<PathBuf> {
        self.workload_toml.clone()
    }

    pub fn bb_tests_root(&self) -> PathBuf {
        self.bb_tests_root.clone()
    }

    #[cfg(feature = "p2e")]
    pub fn p2e_bitstream(&self) -> PathBuf {
        self.p2e_bitstream.clone().expect("--p2e-bitstream is required")
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
