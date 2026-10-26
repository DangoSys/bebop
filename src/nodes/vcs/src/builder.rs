use snafu::{ensure_whatever, ResultExt, Whatever};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Builder {
    pub rtl_dir: PathBuf,
    pub artifact_dir: PathBuf,
    pub jobs: usize,
}

fn rtl_files(directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            rtl_files(&path, files)?;
        } else if matches!(path.extension().and_then(|s| s.to_str()), Some("v" | "sv")) {
            files.push(path);
        }
    }
    Ok(())
}

impl Builder {
    pub fn build(self) -> Result<(), Whatever> {
        let node = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo = node
            .join("../../../..")
            .canonicalize()
            .whatever_context("locate repository")?;
        let rtl = self.rtl_dir.canonicalize().whatever_context("locate VCS RTL")?;
        let mut sources = Vec::new();
        rtl_files(&rtl, &mut sources).whatever_context("read VCS RTL")?;
        sources.sort();
        ensure_whatever!(!sources.is_empty(), "no VCS RTL sources found");
        std::fs::create_dir_all(&self.artifact_dir).whatever_context("create VCS build directory")?;
        let output = self
            .artifact_dir
            .canonicalize()
            .whatever_context("locate VCS build directory")?;
        let dram = repo.join("result/share/dramsim3");
        let arch = repo.join("arch/src/csrc");
        sources.push(node.join("native/BBSimVcsHarness.sv"));
        sources.push(node.join("native/vcs_dpi.cc"));
        for source in ["BBSimDRAM.cc", "mm.cc", "mm_dramsim3.cc"] {
            sources.push(arch.join("src/monitor/ioe").join(source));
        }
        for source in [
            "bankstate.cc",
            "channel_state.cc",
            "command_queue.cc",
            "common.cc",
            "configuration.cc",
            "controller.cc",
            "dram_system.cc",
            "hmc.cc",
            "refresh.cc",
            "simple_stats.cc",
            "timing.cc",
            "memory_system.cc",
        ] {
            sources.push(dram.join("src").join(source));
        }
        let cflags = format!(
            "-std=c++17 -DFMT_HEADER_ONLY -I{} -I{} -I{} -I{} -I{}",
            dram.join("src").display(),
            dram.join("ext/headers").display(),
            dram.join("ext/fmt/include").display(),
            rtl.display(),
            arch.join("include").display()
        );
        let mut command = Command::new("vcs");
        // VCS and its DPI objects use the EDA host ABI. Nix libraries must not
        // introduce a second glibc into the simulator process.
        for variable in [
            "NIX_LDFLAGS",
            "NIX_CFLAGS_COMPILE",
            "NIX_LDFLAGS_FOR_TARGET",
            "NIX_CFLAGS_COMPILE_FOR_TARGET",
            "CPATH",
            "LIBRARY_PATH",
            "C_INCLUDE_PATH",
            "CPLUS_INCLUDE_PATH",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
            "LD_LIBRARY_PATH",
        ] {
            command.env_remove(variable);
        }
        let status = command
            .current_dir(&output)
            .args([
                "-full64",
                "-sverilog",
                "-timescale=1ns/1ps",
                "-cpp",
                "/usr/bin/g++",
                "-cc",
                "/usr/bin/gcc",
                "-ld",
                "/usr/bin/g++",
                "-top",
                "BBSimVcsHarness",
                "-debug_access+all",
                "-hsopt=off",
            ])
            .arg("-j")
            .arg(self.jobs.to_string())
            .arg(format!("-Mdir={}", output.join("csrc").display()))
            .arg("-o")
            .arg(output.join("simv"))
            .arg("-l")
            .arg(output.join("vcs_compile.log"))
            .arg(format!("+incdir+{}", rtl.display()))
            .arg("-CFLAGS")
            .arg(cflags)
            .args(["-LDFLAGS", "-B/usr/bin/", "-syslib", "-lstdc++fs -lstdc++"])
            .args(sources)
            .status()
            .whatever_context("launch VCS compiler")?;
        ensure_whatever!(status.success(), "VCS compilation failed: {status}");
        Ok(())
    }
}
