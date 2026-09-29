use super::{run::Args, Tile};
use crate::{config, root::chip::Chip, Core, TraceConfig};
use std::{sync::Arc, time::Instant};

pub fn run(args: Args) -> Result<(), String> {
    let topology = config::tile_topology(args.tile_index);
    let chip = Chip::new(args.memory_mib << 20, topology.cores.len() + 1);
    Machine::prepare(&args, &chip, 0)?.run(&args, &std::sync::atomic::AtomicBool::new(false), None)
}

pub(crate) struct Machine {
    controller: Core,
    workers: Vec<Core>,
    tile: Arc<Tile>,
}

impl Machine {
    pub(crate) fn prepare(args: &Args, chip: &Chip, first_hart: usize) -> Result<Self, String> {
        let topology = config::tile_topology(args.tile_index);
        let tile = Tile::new(
            &chip,
            first_hart,
            topology.cores.len() + 1,
            topology
                .cores
                .iter()
                .map(|(_, index)| config::core_signature(*index))
                .collect(),
            topology.shared_physical_bank_count,
            topology.shared_bank_size,
            topology.virtual_bank_count,
        );
        let mut controller = Core::new_with_core_hart(
            &args.log_dir.join("hart-0"),
            TraceConfig::new(args.itrace, args.mtrace),
            args.disasm,
            args.profile,
            topology.cores[0].1,
            0,
            Some(Arc::clone(&tile)),
        )
        .map_err(|error| error.to_string())?;
        controller.set_arguments(args.arguments.clone());
        controller
            .load_elf(&args.elf, args.pk)
            .map_err(|error| error.to_string())?;
        controller.set_working_directory(args.log_dir.canonicalize().map_err(|error| error.to_string())?);
        controller.init_hart(args.pk).map_err(|error| error.to_string())?;
        let mut workers = Vec::new();
        for (index, (_, core)) in topology.cores.into_iter().enumerate() {
            workers.push(
                Core::new_with_core_hart(
                    &args.log_dir.join(format!("hart-{}", index + 1)),
                    TraceConfig::new(args.itrace, args.mtrace),
                    args.disasm,
                    args.profile,
                    core,
                    index + 1,
                    Some(Arc::clone(&tile)),
                )
                .map_err(|error| error.to_string())?,
            );
        }
        Ok(Self {
            controller,
            workers,
            tile,
        })
    }

    pub(crate) fn run(self, args: &Args, cancelled: &std::sync::atomic::AtomicBool,
                     endpoint: Option<&crate::root::host_io::Endpoint>) -> Result<(), String> {
        let Self {
            mut controller,
            workers,
            tile,
        } = self;
        std::thread::scope(|scope| {
            let mut threads = Vec::new();
            for (index, mut worker) in workers.into_iter().enumerate() {
                let tile = &tile;
                let log_dir = &args.log_dir;
                threads.push(
                    std::thread::Builder::new()
                        .name(format!("core-{}", index + 1))
                        .spawn_scoped(scope, move || {
                            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                let started = Instant::now();
                                while let Some(task) = tile.tasks.take(index) {
                                    worker.start_task(task);
                                    while !worker.task_done() && !tile.tasks.stopped() {
                                        if let Err(error) = worker.step(256) {
                                            eprintln!("core {}: {error}", index + 1);
                                            worker.complete_task(1);
                                            break;
                                        }
                                    }
                                }
                                worker.stop(0);
                                worker.step(0).map_err(|error| error.to_string())?;
                                if let Some(report) = worker.profile_report(started.elapsed()) {
                                    std::fs::write(
                                        log_dir.join(format!("hart-{}/tool-profile.txt", index + 1)),
                                        crate::format_profile_report(&report),
                                    )
                                    .map_err(|error| error.to_string())?;
                                }
                                Ok::<(), String>(())
                            }));
                            if result.is_err() {
                                tile.tasks.stop();
                            }
                            result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                        })
                        .map_err(|error| {
                            tile.tasks.stop();
                            error.to_string()
                        })?,
                );
            }
            let tile = &tile;
            let controller_thread = std::thread::Builder::new()
                .name("core-0".into())
                .spawn_scoped(scope, move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if let Some(endpoint) = endpoint {
                            controller.set_stdio(endpoint.accept(cancelled)?)?;
                        }
                        while !controller.finished() {
                            if !args.pk {
                                if let Some(code) = tile.platform.lock().expect("BEMU platform poisoned").exit_codes[0]
                                {
                                    return if code == 0 {
                                        Ok(())
                                    } else {
                                        Err(format!("chip firmware exited with code {code}"))
                                    };
                                }
                            }
                            if tile.tasks.stopped() || cancelled.load(std::sync::atomic::Ordering::Acquire) {
                                return Err("NPU core stopped unexpectedly".to_string());
                            }
                            controller.step(256).map_err(|error| error.to_string())?;
                        }
                        let code = controller.exit_code().expect("controller exit status");
                        if code != 0 {
                            return Err(format!("controller exited with code {code}"));
                        }
                        Ok(())
                    }));
                    tile.tasks.stop();
                    (
                        result.unwrap_or_else(|_| Err("controller panicked".to_string())),
                        controller,
                    )
                })
                .map_err(|error| {
                    tile.tasks.stop();
                    error.to_string()
                })?;
            let (result, controller) = controller_thread
                .join()
                .map_err(|_| "controller thread panicked".to_string())?;
            for thread in threads {
                thread.join().map_err(|_| "NPU core panicked".to_string())??;
            }
            drop(controller);
            result
        })
    }
}
