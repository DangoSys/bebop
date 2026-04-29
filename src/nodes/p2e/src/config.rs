use clap::{CommandFactory, Parser};
use snafu::{FromString, Whatever};
use std::path::PathBuf;

pub const OUT_DIR: &str = "./out";
pub const HW_CONFIG: &str = "hw-config.hdf";
pub const VVAC_TOP_MODULE: &str = "xepic_vvac_top";
pub const CTB_FPGA_ID: &str = "P0";

#[derive(Debug, Parser)]
#[command(name = "bebop-p2e", about = "Bebop P2E FPGA flow")]
struct CliArgs {
    #[arg(long, help = "Build the P2E bitstream")]
    buildbitstream: bool,
    #[arg(long, help = "Run the workload through VVAC CTB")]
    runworkload: bool,
}

#[derive(Debug, Clone)]
pub struct P2EOptions {
    pub buildbitstream: bool,
    pub runworkload: bool,
}

impl P2EOptions {
    pub fn out_dir(&self) -> PathBuf {
        PathBuf::from(OUT_DIR)
    }

    pub fn bitstream(&self) -> PathBuf {
        self.out_dir().join("fpgaCompDir/bitstream.bit")
    }

    pub fn rtcfg(&self) -> PathBuf {
        self.out_dir().join("vvacDir/runtimeDir/rtcfg")
    }
}

pub fn parse_args(args: Vec<String>) -> Result<P2EOptions, Whatever> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push("bebop-p2e".to_string());
    argv.extend(args);

    let parsed =
        CliArgs::try_parse_from(argv).map_err(|e| Whatever::without_source(e.to_string()))?;

    let options = P2EOptions {
        buildbitstream: parsed.buildbitstream,
        runworkload: parsed.runworkload,
    };
    validate_tasks(&options)?;
    Ok(options)
}

pub fn help_text() -> String {
    CliArgs::command().render_help().to_string()
}

pub fn validate_tasks(options: &P2EOptions) -> Result<(), Whatever> {
    if options.buildbitstream || options.runworkload {
        Ok(())
    } else {
        Err(Whatever::without_source(
            "choose at least one task: --buildbitstream or --runworkload".to_string(),
        ))
    }
}
