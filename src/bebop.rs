//! CLI dispatch, Spike 子进程启动与 examples 构建入口。

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};
use log::{debug, info};

use crate::emu::config::{BANK_NUM, BANK_SIZE};
use crate::emu::interface::{BemuSpikeInterface, SpikeCallbackParams};
use crate::shm;

const SPIKE_EXT: &str = "--extension=bebop_rocc";

static SPIKE_TESTS: &[&str] = &[
    "test_bemu_custom",
    "test_bemu_mvin_mvout",
    "test_bemu_matmul",
    "test_bemu_transpose",
    "test_bemu_integration",
];

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU CLI")]
pub struct Cli {
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Execute {
        #[arg(short, long)]
        funct: u32,
        #[arg(long, default_value_t = 0)]
        xs1: u64,
        #[arg(long, default_value_t = 0)]
        xs2: u64,
    },
    Info,
    #[command(hide = true)]
    ShmSmoke {
        #[arg(long, default_value_t = 4096)]
        size: usize,
    },
    /// Configure and build RISC-V examples (cmake + ninja), produces ELFs and libbebop_rocc.so.
    Prepare,
    /// Run Spike + pk on built example tests (needs `prepare` first).
    SpikeTest {
        #[arg(long, default_value_t = false)]
        all: bool,
    },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Execute { funct, xs1, xs2 } => cmd_execute(funct, xs1, xs2, cli.verbose),
        Commands::Info => {
            cmd_info();
            Ok(())
        }
        Commands::ShmSmoke { size } => shm::run_smoke(size),
        Commands::Prepare => prepare_examples(),
        Commands::SpikeTest { all } => spike_tests(all),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn examples_build_dir() -> PathBuf {
    repo_root().join("examples").join("build")
}

fn resolve_on_path(cmd: &str) -> Result<PathBuf, String> {
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {cmd}")])
        .output()
        .map_err(|e| format!("failed to run sh for command -v {cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!("'{cmd}' not found in PATH (run `nix develop`?)"));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(format!("'{cmd}' not found in PATH"));
    }
    Ok(PathBuf::from(s))
}

fn spike_lib_dir(spike_exe: &Path) -> Result<PathBuf, String> {
    let bin_dir = spike_exe
        .parent()
        .ok_or_else(|| "spike path has no parent".to_string())?;
    let root = bin_dir
        .parent()
        .ok_or_else(|| "spike install layout unexpected".to_string())?;
    Ok(root.join("lib"))
}

fn prepare_examples() -> Result<(), String> {
    let root = repo_root();
    let examples = root.join("examples");
    let build = examples_build_dir();
    if !examples.is_dir() {
        return Err(format!("examples dir missing: {}", examples.display()));
    }
    info!("cmake: {} -> {}", examples.display(), build.display());
    let st = Command::new("cmake")
        .args([
            "-S",
            examples.to_str().ok_or("examples path is not UTF-8")?,
            "-B",
            build.to_str().ok_or("build path is not UTF-8")?,
            "-G",
            "Ninja",
        ])
        .status()
        .map_err(|e| format!("failed to run cmake: {e}"))?;
    if !st.success() {
        return Err("cmake failed".into());
    }
    let mut ninja = Command::new("ninja");
    ninja.arg("-C").arg(&build);
    for t in SPIKE_TESTS {
        ninja.arg(*t);
    }
    ninja.args(["libbemu", "bebop_rocc"]);
    info!("ninja: build targets in {}", build.display());
    let st = ninja
        .status()
        .map_err(|e| format!("failed to run ninja: {e}"))?;
    if !st.success() {
        return Err("ninja failed".into());
    }
    Ok(())
}

fn spike_tests(all: bool) -> Result<(), String> {
    let root = repo_root();
    let build = examples_build_dir();
    let spike = resolve_on_path("spike")?;
    let pk = resolve_on_path("pk")?;
    let spike_lib = spike_lib_dir(&spike)?;
    let bemu_lib = root.join("target/release/libbemu.so");
    if !bemu_lib.is_file() {
        return Err(format!(
            "missing {} — run `cargo build --release` or `bebop prepare`",
            bemu_lib.display()
        ));
    }
    let rocc = build.join("libbebop_rocc.so");
    if !rocc.is_file() {
        return Err(format!("missing {} — run `bebop prepare`", rocc.display()));
    }

    let ld = ld_library_path(&root, &spike_lib, &build);
    let list: &[&str] = if all { SPIKE_TESTS } else { &SPIKE_TESTS[..1] };

    for exe in list {
        let elf = build.join(exe);
        if !elf.is_file() {
            return Err(format!("missing {} — run `bebop prepare`", elf.display()));
        }
        run_spike_pk(&spike, &pk, &elf, &ld)?;
    }
    Ok(())
}

fn ld_library_path(root: &Path, spike_lib: &Path, example_build: &Path) -> String {
    let mut parts = vec![
        root.join("target/release").display().to_string(),
        spike_lib.display().to_string(),
        example_build.display().to_string(),
    ];
    if let Ok(prev) = env::var("LD_LIBRARY_PATH") {
        if !prev.is_empty() {
            parts.push(prev);
        }
    }
    parts.join(":")
}

fn run_spike_pk(spike: &Path, pk: &Path, elf: &Path, ld_library_path: &str) -> Result<(), String> {
    debug!(
        "LD_LIBRARY_PATH={} {} {} {} {}",
        ld_library_path,
        spike.display(),
        SPIKE_EXT,
        pk.display(),
        elf.display()
    );
    info!("spike: {}", elf.display());
    let st = Command::new(spike)
        .arg(SPIKE_EXT)
        .arg(pk)
        .arg(elf)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .status()
        .map_err(|e| format!("failed to spawn spike: {e}"))?;
    if !st.success() {
        return Err(format!("spike exited with {:?}", st.code()));
    }
    Ok(())
}

fn cmd_execute(funct: u32, xs1: u64, xs2: u64, verbose: bool) -> Result<(), String> {
    let mut itf = BemuSpikeInterface::with_verbose(verbose);
    let res = itf
        .handle_custom_instruction(&SpikeCallbackParams::new(funct, xs1, xs2))
        .map_err(|e| format!("execute failed: {e}"))?;
    println!("result=0x{res:x}");
    Ok(())
}

fn cmd_info() {
    println!("bebop 0.1.0");
    println!("banks={BANK_NUM}, bank_size={}KB", BANK_SIZE / 1024);
    println!("supported funct: 23(mset) 24(mvin) 25(mvout) 32(matmul) 34(transpose)");
}
