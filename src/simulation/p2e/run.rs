#[cfg(feature = "p2e")]
use snafu::FromString;
use snafu::Whatever;
#[cfg(feature = "p2e")]
use std::path::PathBuf;
#[cfg(feature = "p2e")]
use std::time::Instant;

#[cfg(feature = "p2e")]
use bebop_p2e::{self};
#[cfg(feature = "p2e")]
use bebop_rtl_trace::{init_trace, write_trace_summary, TraceConfig};
#[cfg(feature = "p2e")]
use bebop_uart::{ConsoleConfig, ConsoleServer};
#[cfg(feature = "p2e")]
use snafu::ResultExt;

#[cfg(all(feature = "p2e", feature = "bemu"))]
use crate::simulation::lib::difftest::DiffSession;

#[cfg(feature = "p2e")]
pub struct P2eRunConfig {
    pub image: Option<PathBuf>,
    pub load_manifest: Option<PathBuf>,
    pub bitstream: PathBuf,
    pub log_dir: PathBuf,
    pub fpga_location: String,
    pub multi_fpga: bool,
    pub wave: bool,
    pub wave_start: Option<u64>,
    pub diff: Option<DiffConfig>,
    pub trace: P2eTraceConfig,
}

#[derive(Debug)]
#[cfg(feature = "p2e")]
pub struct DiffConfig {
    pub image_elf: PathBuf,
}

#[derive(Debug)]
#[cfg(feature = "p2e")]
pub struct P2eTraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
}

#[cfg(feature = "p2e")]
pub fn run(config: P2eRunConfig) -> Result<(), Whatever> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();
    let command_started = Instant::now();

    #[cfg(not(feature = "bemu"))]
    if config.diff.is_some() {
        return Err(Whatever::without_source(
            "this executable was built without BEMU; rebuild P2E with --diff".to_string(),
        ));
    }

    if !config.bitstream.exists() {
        snafu::whatever!("bitstream not found: {}", config.bitstream.display());
    }
    if let Some(diff) = &config.diff {
        if !diff.image_elf.exists() {
            snafu::whatever!("P2E DiffTest image ELF not found: {}", diff.image_elf.display());
        }
    }
    if !config.log_dir.exists() {
        snafu::whatever!("log directory not found: {}", config.log_dir.display());
    }

    let bitstream = config
        .bitstream
        .canonicalize()
        .whatever_context("failed to canonicalize P2E bitstream")?;
    let case_home = bitstream
        .parent()
        .and_then(|fpga_comp_dir| fpga_comp_dir.parent())
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| Whatever::without_source("P2E bitstream must be under <case>/fpgaCompDir".to_string()))?;
    let rtcfg_path = case_home.join("vvacDir/runtimeDir/rtcfg");
    if config.trace.itrace || config.trace.mtrace {
        let mode = std::fs::read_to_string(case_home.join("p2e_trace_mode"))
            .whatever_context("failed to read P2E trace capabilities")?;
        for (trace, requested) in [("itrace", config.trace.itrace), ("mtrace", config.trace.mtrace)] {
            if requested && !mode.trim().split('+').any(|capability| capability == trace) {
                snafu::whatever!("P2E case does not support {trace}; rebuild with --{trace}");
            }
        }
    }
    if config.load_manifest.is_some() && config.diff.is_some() {
        snafu::whatever!("multi-segment load manifests do not support single-ELF DiffTest");
    }
    let load_plan = bebop_p2e::validate_loads(config.image.as_deref(), config.load_manifest.as_deref())
        .whatever_context("invalid P2E load inputs")?;
    let load_record = serde_json::to_vec_pretty(&load_plan).whatever_context("failed to encode load record")?;
    std::fs::write(config.log_dir.join("load-inputs.json"), load_record)
        .whatever_context("failed to save P2E load input identity")?;
    if !rtcfg_path.exists() {
        snafu::whatever!("P2E runtime config not found: {}", rtcfg_path.display());
    }

    let uart_log_path = config.log_dir.join("uart.log");

    log::info!("P2E Simulation Starting");
    log::info!("  Load inputs: {:?}", load_plan);
    log::info!("  Bitstream: {}", bitstream.display());
    log::info!("  FPGA: {}", config.fpga_location);
    log::info!("  Runtime case: {}", case_home.display());
    log::info!("  Log directory: {}", config.log_dir.display());
    log::info!("  UART Log: {}", uart_log_path.display());
    log::info!("  Multi FPGA: {}", config.multi_fpga);
    log::info!("  Waveform: {}", config.wave);
    log::info!("  Waveform Start Cycle: {}", config.wave_start.unwrap_or(0));
    log::info!("  Bank DiffTest: {}", config.diff.is_some());
    log::info!("  Trace: {:?}", config.trace);

    bebop_p2e::source_environment().whatever_context("failed to initialize P2E environment")?;
    bebop_p2e::configure_vvac_environment();
    bebop_p2e::ffi::reset_runtime_state();
    bebop_p2e::ffi::set_log_dir(config.log_dir.to_string_lossy().to_string());
    bebop_p2e::ffi::init_cycle_trace(&config.log_dir)
        .map_err(|e| Whatever::without_source(format!("failed to initialize P2E cycle trace collector: {e}")))?;
    init_trace(
        &config.log_dir,
        TraceConfig {
            itrace: config.trace.itrace,
            mtrace: config.trace.mtrace,
            pmctrace: config.trace.pmctrace,
            ctrace: config.trace.ctrace,
            banktrace: config.trace.banktrace,
        },
    )
    .map_err(|e| Whatever::without_source(format!("failed to init P2E trace: {e}")))?;

    let console = ConsoleServer::start(&config.log_dir, ConsoleConfig::new("p2e"), bebop_p2e::ffi::push_uart_rx)
        .whatever_context("failed to start P2E console")?;
    bebop_p2e::ffi::set_console_tx(console.tx_sender());

    std::env::set_current_dir(&case_home).whatever_context("failed to enter P2E case directory")?;

    let main_tcl = bebop_p2e::generate_main_tcl(
        &config.fpga_location,
        &load_plan,
        &bitstream,
        config.multi_fpga,
        config.wave,
        config.wave_start.unwrap_or(0),
    )
    .whatever_context("failed to generate P2E main.tcl")?;
    let main_tcl_path = case_home.join("main.tcl");
    std::fs::write(&main_tcl_path, main_tcl).whatever_context("failed to write P2E main.tcl")?;

    let flash_done_flag = case_home.join("flash_done.flag");
    let host_init_flag = case_home.join("host_init_done.flag");
    let sim_exit_flag = case_home.join("sim_exit.flag");
    let _ = std::fs::remove_file(&flash_done_flag);
    let _ = std::fs::remove_file(&host_init_flag);
    let _ = std::fs::remove_file(&sim_exit_flag);

    let mut vdbg = bebop_p2e::start_vdbg_background(&main_tcl_path).whatever_context("failed to start P2E vdbg")?;
    bebop_p2e::wait_for_flash(&flash_done_flag, &mut vdbg, || Ok(())).whatever_context("P2E flash failed")?;

    let mut vdbg = vdbg.exit_on_drop(sim_exit_flag.clone());
    let ctb = bebop_p2e::init_ctb(&case_home, &rtcfg_path).whatever_context("failed to initialize P2E CTB")?;
    #[cfg(feature = "bemu")]
    let diff_started = config.diff.as_ref().map(|_| Instant::now());
    #[cfg(feature = "bemu")]
    let mut diff_session = config
        .diff
        .as_ref()
        .map(|diff| DiffSession::new(&diff.image_elf, &config.log_dir))
        .transpose()?;
    std::fs::write(&host_init_flag, "").whatever_context("failed to signal P2E host init")?;

    let result = bebop_p2e::wait_for_completion(|| {
        if !bebop_p2e::ffi::check_exit() {
            vdbg.check_running()?;
        }
        #[cfg(feature = "bemu")]
        if let Some(diff) = diff_session.as_mut() {
            diff.sync_golden(None).map_err(|error| error.to_string())?;
        }
        Ok(())
    })
    .map_err(|error| Whatever::without_source(format!("P2E simulation failed: {error}")))?;
    let executable_timing = bebop_p2e::ffi::executable_timing();
    let simulation_finished = Instant::now();
    #[cfg(feature = "bemu")]
    if let Some(diff) = diff_session.as_mut() {
        let drain_started = Instant::now();
        let drain_deadline = drain_started + std::time::Duration::from_secs(60);
        let mut expected = None;
        let mut snapshots = Vec::new();
        loop {
            if expected.is_none() {
                snapshots = ctb
                    .btrace_snapshots(&case_home)
                    .map_err(|e| Whatever::without_source(format!("failed to query BTrace: {e}")))?;
                if snapshots.iter().all(|s| s.idle) {
                    let mut counts = std::collections::BTreeMap::new();
                    for snapshot in &snapshots {
                        if counts.insert(snapshot.hart_id, snapshot.produced).is_some() {
                            snafu::whatever!("duplicate BTrace hart {}", snapshot.hart_id);
                        }
                    }
                    expected = Some(counts);
                }
            }
            if let Some(counts) = &expected {
                if diff.drain_status(counts, drain_deadline)? {
                    println!(
                        "BTrace drain completed: expected={counts:?}, elapsed={:.3} s",
                        drain_started.elapsed().as_secs_f64()
                    );
                    break;
                }
            } else {
                diff.sync_golden(Some(drain_deadline))?;
            }
            if drain_started.elapsed() >= std::time::Duration::from_secs(60) {
                snafu::whatever!(
                    "BTrace drain timed out: hardware={snapshots:?}, received={:?}, comparison={:?}",
                    bebop_bank_hash::subject_counts(),
                    bebop_bank_hash::progress()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    #[cfg(feature = "bemu")]
    let (diff_passed, diff_elapsed) = if let Some(mut diff) = diff_session {
        diff.sync_golden(None)?;
        diff.finish()?;
        (true, Some(diff_started.expect("diff session start exists").elapsed()))
    } else {
        (false, None)
    };
    let diff_finished = Instant::now();
    drop(console);
    bebop_p2e::ffi::finish_cycle_trace()
        .map_err(|e| Whatever::without_source(format!("failed to finalize P2E cycle trace: {e}")))?;
    write_trace_summary(&config.log_dir).whatever_context("failed to write P2E RTL trace summary")?;

    std::fs::write(&uart_log_path, &result.uart_log).whatever_context("failed to write P2E UART log")?;

    log::info!("P2E simulation completed");
    log::info!("  Exit code: {}", result.exit_code);
    log::info!("  Elapsed: {:?}", result.elapsed);
    log::info!("  UART log: {}", uart_log_path.display());

    if !result.uart_log.is_empty() {
        println!("\n=== UART Output ===");
        println!("{}", result.uart_log);
    }

    #[cfg(feature = "bemu")]
    if diff_passed {
        println!("Bank DiffTest passed");
    }

    let command_finished = Instant::now();
    let mut timing_summary = String::from("P2E timing summary (host wall-clock, additive phases):\n");
    if let Some((name, executable_started, executable_finished)) = executable_timing {
        let before_executable = executable_started.duration_since(command_started);
        let executable_elapsed = executable_finished.duration_since(executable_started);
        let after_executable = simulation_finished.duration_since(executable_finished);
        let diff_finalize = diff_finished.duration_since(simulation_finished);
        let output_cleanup = command_finished.duration_since(diff_finished);
        timing_summary.push_str(&format!(
            "  P2E setup/Linux boot: {:.3} s\n  Linux executable {name}: {:.3} s\n  FPGA completion: {:.3} s\n",
            before_executable.as_secs_f64(),
            executable_elapsed.as_secs_f64(),
            after_executable.as_secs_f64()
        ));
        #[cfg(feature = "bemu")]
        if config.diff.is_some() {
            timing_summary.push_str(&format!(
                "  Bank DiffTest finalization: {:.3} s\n",
                diff_finalize.as_secs_f64()
            ));
        } else {
            timing_summary.push_str(&format!("  Trace finalization: {:.3} s\n", diff_finalize.as_secs_f64()));
        }
        #[cfg(not(feature = "bemu"))]
        timing_summary.push_str(&format!("  Trace finalization: {:.3} s\n", diff_finalize.as_secs_f64()));
        timing_summary.push_str(&format!(
            "  Output cleanup: {:.3} s\n  Total P2E command: {:.3} s\n",
            output_cleanup.as_secs_f64(),
            command_finished.duration_since(command_started).as_secs_f64()
        ));
    } else {
        timing_summary.push_str(&format!(
            "  Linux executable timing unavailable: RUN/PASS markers not observed\n  FPGA workload: {:.3} s\n  Total P2E command: {:.3} s\n",
            result.elapsed.as_secs_f64(),
            command_finished.duration_since(command_started).as_secs_f64()
        ));
    }
    #[cfg(feature = "bemu")]
    if let Some(elapsed) = diff_elapsed {
        timing_summary.push_str(&format!("  Bank DiffTest session: {:.3} s\n", elapsed.as_secs_f64()));
    }
    print!("{timing_summary}");
    let timing_log_path = config.log_dir.join("p2e_timing.log");
    std::fs::write(&timing_log_path, timing_summary).whatever_context("failed to write P2E timing log")?;
    println!("  Timing log: {}", timing_log_path.display());

    if result.exit_code != 0 {
        snafu::whatever!("P2E exited with code {}", result.exit_code);
    }
    Ok(())
}

#[cfg(not(feature = "p2e"))]
pub fn run_unavailable() -> Result<(), Whatever> {
    snafu::whatever!("p2e runner is not compiled into this executable");
}
