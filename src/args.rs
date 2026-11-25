use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "bebop", about = "Bebop CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Install a Buckyball design configuration for this Bebop checkout.
    Config {
        #[arg(long)]
        design: String,
    },
    /// Build a simulator artifact.
    Build(BuildCommand),
    /// Run a workload on a built simulator artifact.
    Run(RunCommand),
    #[command(about = "Disassemble DASM records from standard input")]
    Dasm,
}

#[derive(Debug, Args)]
pub struct BuildCommand {
    #[command(subcommand)]
    pub target: BuildTarget,
}

#[derive(Debug, Subcommand)]
pub enum BuildTarget {
    /// Build the VCS SoC simulator.
    Vcs {
        #[arg(long)]
        rtl_dir: PathBuf,
        #[arg(long)]
        artifact_dir: PathBuf,
        #[arg(long, default_value = "16")]
        jobs: usize,
    },
    /// Build the selected design's BEMU runner.
    Bemu {
        #[arg(long)]
        design_dir: PathBuf,
        #[arg(
            long,
            value_name = "DIR",
            help = "The simulation entry's emu crate, which builds the runner"
        )]
        entry_dir: PathBuf,
    },
    /// Build a Verilator-based simulator.
    Verilator {
        #[arg(long, value_name = "DESIGN_DIR")]
        design_dir: PathBuf,
        #[arg(
            long,
            value_name = "DIR",
            help = "The simulation entry's emu crate, which builds the runner"
        )]
        entry_dir: PathBuf,
        #[arg(long, value_name = "DIR")]
        rtl_dir: PathBuf,
        #[arg(long, help = "Build a Verilator+BEMU difftest executable")]
        diff: bool,
        #[arg(long, help = "Build a Verilator+BEMU difftest executable")]
        fast: bool,
    },
    /// Build a P2E simulator artifact.
    P2e {
        #[arg(long, value_name = "DESIGN_DIR")]
        design_dir: PathBuf,
        #[arg(
            long,
            value_name = "DIR",
            help = "The simulation entry's emu crate, which builds the runner"
        )]
        entry_dir: PathBuf,
        #[arg(long, value_name = "DIR")]
        rtl_dir: PathBuf,
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
        #[arg(long, help = "Build a P2E+BEMU Bank DiffTest executable")]
        diff: bool,
        #[arg(long, help = "Include NPU instruction trace in the P2E case")]
        itrace: bool,
        #[arg(long, help = "Include NPU memory trace in the P2E case")]
        mtrace: bool,
    },
}

#[derive(Debug, Args)]
pub struct RunCommand {
    #[command(subcommand)]
    pub target: RunTarget,
}

#[derive(Debug, Subcommand)]
pub enum RunTarget {
    /// Run a SoC workload with VCS.
    Vcs(VcsArgs),
    /// Run a workload on a Verilator-based simulator artifact.
    Verilator(VerilatorArgs),
    /// Run a workload on a P2E simulator artifact.
    P2e(P2eArgs),
}

#[derive(Debug, Parser)]
#[command(name = "bebop-vcs")]
pub struct VcsArgs {
    #[arg(long)]
    pub simv: PathBuf,
    #[arg(long)]
    pub elf: PathBuf,
    #[arg(long)]
    pub dram_configs: PathBuf,
    #[arg(long)]
    pub log_dir: PathBuf,
    #[arg(long, num_args = 0..=1, default_missing_value = "vpd", value_parser = ["vcd", "vpd"])]
    pub wave: Option<String>,
}

#[derive(Debug, Parser)]
#[command(name = "bebop-verilator")]
pub struct VerilatorArgs {
    #[arg(long, value_name = "ELF")]
    pub elf: PathBuf,
    #[arg(long, value_name = "DIR")]
    pub log_dir: PathBuf,
    #[arg(long, help = "Disable waveform dump")]
    pub no_wave: bool,
    #[arg(long, help = "Run with a BEMU difftest instance")]
    pub diff: bool,
    #[arg(long, help = "Run with a BEMU fast/difftest instance")]
    pub fast: bool,
    #[arg(long, help = "Enable RTL instruction trace")]
    pub itrace: bool,
    #[arg(long, help = "Enable RTL memory trace")]
    pub mtrace: bool,
    #[arg(long, help = "Enable RTL performance counter trace")]
    pub pmctrace: bool,
    #[arg(long, help = "Enable RTL cycle counter trace")]
    pub ctrace: bool,
    #[arg(long, help = "Enable RTL bank trace")]
    pub banktrace: bool,
    #[arg(long, help = "Print the CPU commit log (+disasm=1) into disasm.log")]
    pub disasm: bool,
}

#[derive(Debug, Parser)]
#[command(name = "bebop-p2e")]
pub struct P2eArgs {
    #[arg(long, value_name = "IMAGE")]
    pub image: PathBuf,
    /// A model image and its byte offset in the DDR (the model region).
    #[arg(long, value_name = "FILE", requires = "model_offset")]
    pub model_image: Option<PathBuf>,
    #[arg(long, value_name = "BYTES", requires = "model_image")]
    pub model_offset: Option<u64>,
    #[arg(long, value_name = "BIT")]
    pub bitstream: PathBuf,
    #[arg(long, value_name = "DIR")]
    pub log_dir: PathBuf,
    #[arg(long, default_value = "0.A", value_name = "LOCATION")]
    pub fpga_location: String,
    #[arg(long, help = "Use multi-FPGA hw_server connection without a location selector")]
    pub multi_fpga: bool,
    #[arg(long, help = "Enable waveform dump")]
    pub wave: bool,
    #[arg(long, help = "Start waveform dump from this cycle")]
    pub wave_start: Option<u64>,
    #[arg(long, requires = "image_elf", help = "Compare P2E bank state against BEMU")]
    pub diff: bool,
    #[arg(long, value_name = "ELF", help = "ELF corresponding to --image")]
    pub image_elf: Option<PathBuf>,
    #[arg(
        long,
        requires = "diff",
        value_name = "FILE",
        help = "DDR image loaded before the ELF in the BEMU golden model"
    )]
    pub ddr_image: Option<PathBuf>,
    #[arg(long, help = "Enable RTL instruction trace")]
    pub itrace: bool,
    #[arg(long, help = "Enable RTL memory trace")]
    pub mtrace: bool,
    #[arg(long, help = "Enable RTL performance counter trace")]
    pub pmctrace: bool,
    #[arg(long, help = "Enable RTL cycle counter trace")]
    pub ctrace: bool,
    #[arg(long, help = "Enable RTL bank trace")]
    pub banktrace: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p2e_golden_ddr_image_is_explicit_and_requires_diff() {
        let base = [
            "p2e",
            "--image",
            "firmware.hex",
            "--bitstream",
            "design.bit",
            "--log-dir",
            "run",
        ];
        assert!(P2eArgs::try_parse_from(base.iter().copied().chain(["--ddr-image", "weights.bin"])).is_err());
        let elf_only =
            P2eArgs::try_parse_from(base.iter().copied().chain(["--diff", "--image-elf", "native.elf"])).unwrap();
        assert!(elf_only.ddr_image.is_none());
        let explicit = P2eArgs::try_parse_from(base.iter().copied().chain([
            "--diff",
            "--image-elf",
            "native.elf",
            "--ddr-image",
            "base-weights.bin",
        ]))
        .unwrap();
        assert_eq!(
            explicit.ddr_image.as_deref(),
            Some(std::path::Path::new("base-weights.bin"))
        );
    }
}
