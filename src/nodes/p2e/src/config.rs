use clap::Parser;
use snafu::{whatever, Whatever};
use std::path::PathBuf;

pub const OUT_DIR: &str = "./out";
pub const HW_CONFIG: &str = "hw-config.hdf";
pub const VVAC_TOP_MODULE: &str = "xepic_vvac_top";
pub const CTB_FPGA_ID: &str = "P0";

#[derive(Debug, Parser)]
#[command(name = "bebop-p2e", about = "Bebop P2E FPGA flow")]
pub struct CliArgs {
    #[arg(
        long,
        help = "Build bitstream for the specified config (e.g., sims.p2e.P2EToyConfig)"
    )]
    pub buildbitstream: Option<String>,

    #[arg(long, help = "Run workload on FPGA")]
    pub runworkload: bool,
}

/// Configuration for P2E bitstream builder
#[derive(Debug, Clone)]
pub struct BitstreamConfig {
    pub arch_config: String,
    pub vvac_top_module: String,
    pub output_dir: PathBuf,
    pub hw_config: PathBuf,
    pub vcom_tcl: PathBuf,
}

impl BitstreamConfig {
    /// Create new config with required parameters
    ///
    /// # Arguments
    /// * `arch_config` - Architecture config (e.g., "sims.p2e.P2EToyConfig")
    /// * `vcom_tcl` - Path to vcom_compile.tcl (required, no default)
    pub fn new(arch_config: impl Into<String>, vcom_tcl: impl Into<PathBuf>) -> Self {
        Self {
            arch_config: arch_config.into(),
            vvac_top_module: VVAC_TOP_MODULE.to_string(),
            output_dir: PathBuf::from(OUT_DIR),
            hw_config: PathBuf::from(HW_CONFIG),
            vcom_tcl: vcom_tcl.into(),
        }
    }

    pub fn output_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.output_dir = path.into();
        self
    }

    pub fn hw_config(mut self, path: impl Into<PathBuf>) -> Self {
        self.hw_config = path.into();
        self
    }

    pub fn vcom_tcl(mut self, path: impl Into<PathBuf>) -> Self {
        self.vcom_tcl = path.into();
        self
    }
}

pub fn parse_args() -> Result<CliArgs, Whatever> {
    let args = CliArgs::parse();

    if args.buildbitstream.is_none() && !args.runworkload {
        whatever!("choose at least one task: --buildbitstream=<config> or --runworkload");
    }

    Ok(args)
}
