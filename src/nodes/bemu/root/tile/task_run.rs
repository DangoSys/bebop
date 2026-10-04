use super::{run::Args, Tile};
use crate::{config, root::chip::Chip, Core, TraceConfig};
use std::{sync::Arc, time::Instant};

pub fn run(args: Args) -> Result<(), String> {
    if !args.pk { return super::run::run(args); }
    let chip = Chip::new(args.memory_mib << 20, config::hart_capacity());
    Machine::prepare(&args, &chip)?.run(&args, &std::sync::atomic::AtomicBool::new(false), None)
}

pub(crate) struct Machine {
    controller: Core,
    workers: Vec<Core>,
    tile: Arc<Tile>,
}

impl Machine {
    pub(crate) fn prepare(args: &Args, chip: &Chip) -> Result<Self, String> {
        let topology = config::tile_topology(args.tile_index);
        let controller_core = topology.controller_core.ok_or_else(||
            "PK task execution requires an explicit Tile controller".to_string())?;
        let tile = Tile::new(chip, &topology,
            topology.worker_cores.iter().map(|(_, index)| config::core_signature(*index)).collect());
        let mut controller = Core::new_with_core_hart(
            &args.log_dir.join(format!("hart-{}", config::core_hart_id(controller_core))),
            TraceConfig::new(args.itrace, args.mtrace),
            args.disasm,
            args.profile,
            controller_core,
            config::core_hart_id(controller_core),
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
        for (_, core) in topology.worker_cores {
            workers.push(
                Core::new_with_core_hart(
                    &args.log_dir.join(format!("hart-{}", config::core_hart_id(core))),
                    TraceConfig::new(args.itrace, args.mtrace),
                    args.disasm,
                    args.profile,
                    core,
                    config::core_hart_id(core),
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

    pub(crate) fn run(
        self,
        args: &Args,
        cancelled: &std::sync::atomic::AtomicBool,
        endpoint: Option<&crate::root::host_io::Endpoint>,
    ) -> Result<(), String> {
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
                                        worker
                                            .step(256)
                                            .map_err(|error| format!("core {}: {error}", index + 1))?;
                                    }
                                }
                                worker.stop(0);
                                worker.step(0).map_err(|error| error.to_string())?;
                                if let Some(report) = worker.profile_report(started.elapsed()) {
                                    std::fs::write(
                                        log_dir.join(format!("hart-{}/tool-profile.txt", worker.hart_id())),
                                        crate::format_profile_report(&report),
                                    )
                                    .map_err(|error| error.to_string())?;
                                }
                                Ok::<(), String>(())
                            }));
                            if let Ok(Err(error)) = &result {
                                eprintln!("{error}");
                            }
                            if !matches!(&result, Ok(Ok(()))) {
                                tile.tasks.stop();
                                if let Some(endpoint) = endpoint {
                                    endpoint.stop();
                                }
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
                                let code = tile.exit_codes[controller.hart_id()].load(std::sync::atomic::Ordering::Acquire);
                                if code != i64::MIN {
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
                            controller.wait_control();
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
