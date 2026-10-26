use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Simulator {
    pub executable: PathBuf,
    pub dram_configs: PathBuf,
}

impl Simulator {
    pub fn run(self, elf: &Path, log_dir: &Path, wave: Option<&str>) -> io::Result<()> {
        std::fs::create_dir_all(log_dir)?;
        let mut command = Command::new(self.executable.canonicalize()?);
        command
            .current_dir(log_dir)
            .env_remove("LD_LIBRARY_PATH")
            .arg("+permissive")
            .arg(format!("+elf={}", elf.canonicalize()?.display()))
            .arg("+batch")
            .arg(format!(
                "+dramsim_ini_dir={}",
                self.dram_configs.canonicalize()?.display()
            ))
            .arg(format!(
                "+dramsim_outdir={}",
                log_dir.canonicalize()?.join("dramsim").display()
            ));
        if let Some(format) = wave {
            if !matches!(format, "vcd" | "vpd") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown VCS waveform format",
                ));
            }
            let directory = log_dir.join("waveform");
            std::fs::create_dir_all(&directory)?;
            command.arg(format!(
                "+{format}={}",
                directory.canonicalize()?.join(format!("activity.{format}")).display()
            ));
        }
        command.arg("+permissive-off");
        // Replace the runner so exit status, signals and regression timeouts
        // apply to the simulator itself, without an orphaned child process.
        Err(command.exec())
    }
}
