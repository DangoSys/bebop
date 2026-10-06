use super::Tile;
use crate::root::chip::Chip;
use crate::{config::tile_topology, Core, TraceConfig};
use clap::Parser;
use std::{path::PathBuf, sync::Arc, time::Instant};

#[derive(Parser, Clone)]
pub struct Args {
    #[arg(long, default_value_t = 3072)]
    pub(crate) memory_mib: usize,
    #[arg(long, default_value_t = 0)]
    pub(crate) tile_index: usize,
    #[arg(long)]
    pub(crate) elf: PathBuf,
    #[arg(long)]
    pub(crate) log_dir: PathBuf,
    #[arg(long, conflicts_with = "tile_index")]
    pub(crate) system: bool,
    #[arg(long, requires = "system")]
    pub(crate) dtb: Option<PathBuf>,
    #[arg(long, requires = "system")]
    pub(crate) initrd: Option<PathBuf>,
    #[arg(long)]
    pub(crate) disasm: bool,
    #[arg(long = "tool-profile")]
    pub(crate) profile: bool,
    #[arg(long)]
    pub(crate) itrace: bool,
    #[arg(long)]
    pub(crate) mtrace: bool,
    #[arg(last = true)]
    pub(crate) arguments: Vec<String>,
}

pub fn run(args: Args) -> Result<(), String> {
    if args.system { return crate::root::chip::run_system(&args); }
    let topology = tile_topology(args.tile_index);
    let chip = Chip::new(args.memory_mib << 20, crate::config::hart_capacity());
    chip.clint.coordinate(topology.cores.iter()
        .map(|(_, index)| crate::config::core_hart_id(*index)).collect());
    let signatures = if topology.controller_core.is_some() {
        topology.worker_cores.iter().map(|(_, index)| crate::config::core_signature(*index)).collect()
    } else { Vec::new() };
    let memory = Tile::new(&chip, &topology, signatures);
    let barrier_participants = topology.endpoint_cores.len();
    let mut cores = Vec::with_capacity(topology.cores.len());
    for (_, core_index) in topology.cores {
        let mut core = Core::new_with_core_hart(
            &args.log_dir.join(format!("hart-{}", crate::config::core_hart_id(core_index))),
            TraceConfig::new(args.itrace, args.mtrace),
            args.disasm,
            args.profile,
            core_index,
            crate::config::core_hart_id(core_index),
            Some(Arc::clone(&memory)),
        )
        .map_err(|error| error.to_string())?;
        core.set_arguments(args.arguments.clone());
        core.load_elf(&args.elf).map_err(|error| error.to_string())?;
        core.init_hart().map_err(|error| error.to_string())?;
        cores.push(core);
    }
    let sync = Sync {
        state: std::sync::Mutex::new((0_usize, 0_u64)),
        ready: std::sync::Condvar::new(),
        stopped: std::sync::atomic::AtomicBool::new(false),
        core_count: barrier_participants,
    };
    std::thread::scope(|scope| {
        let mut threads = Vec::with_capacity(cores.len());
        for (local_id, mut core) in cores.into_iter().enumerate() {
            let sync = &sync;
            let args = &args;
            threads.push(
                std::thread::Builder::new()
                    .name(format!("core-{local_id}"))
                    .spawn_scoped(scope, move || {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let started = Instant::now();
                            while !sync.stopped.load(std::sync::atomic::Ordering::Acquire) {
                                core.step(256).map_err(|error| error.to_string())?;
                                if let Some(code) = core.exit_code() {
                                    if code != 0 {
                                        return Err(format!("core {local_id}: guest exited with code {code}"));
                                    }
                                    break;
                                }
                                if core.take_barrier() {
                                    let mut state = sync.state.lock().expect("tile barrier poisoned");
                                    let generation = state.1;
                                    state.0 += 1;
                                    if state.0 == sync.core_count {
                                        state.0 = 0;
                                        state.1 += 1;
                                        sync.ready.notify_all();
                                    } else {
                                        while state.1 == generation
                                            && !sync.stopped.load(std::sync::atomic::Ordering::Acquire)
                                        {
                                            state = sync.ready.wait(state).expect("tile barrier poisoned");
                                        }
                                    }
                                }
                            }
                            core.stop(0);
                            core.step(0).map_err(|error| error.to_string())?;
                            if let Some(report) = core.profile_report(started.elapsed()) {
                                std::fs::write(
                                    args.log_dir.join(format!("hart-{}/tool-profile.txt", core.hart_id())),
                                    crate::format_profile_report(&report),
                                )
                                .map_err(|error| error.to_string())?;
                            }
                            Ok(())
                        }));
                        let _state = sync.state.lock().expect("tile barrier poisoned");
                        sync.stopped.store(true, std::sync::atomic::Ordering::Release);
                        sync.ready.notify_all();
                        drop(_state);
                        result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                    })
                    .map_err(|error| {
                        let _state = sync.state.lock().expect("tile barrier poisoned");
                        sync.stopped.store(true, std::sync::atomic::Ordering::Release);
                        sync.ready.notify_all();
                        error.to_string()
                    })?,
            );
        }
        let mut failure = None;
        for (id, thread) in threads.into_iter().enumerate() {
            let result = thread.join().unwrap_or_else(|_| Err(format!("core {id} panicked")));
            if let Err(error) = result {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    })
}

struct Sync {
    state: std::sync::Mutex<(usize, u64)>,
    ready: std::sync::Condvar,
    stopped: std::sync::atomic::AtomicBool,
    core_count: usize,
}
