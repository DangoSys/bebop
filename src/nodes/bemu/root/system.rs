use super::{
    chip::Chip,
    interconnect::Fabric,
    tile::{run::Args, task_run::Machine},
};
use crate::config;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn run(args: Args, chip_count: usize, vm_chip: Option<usize>, host_io: bool) -> Result<(), String> {
    if chip_count == 0 {
        return Err("system requires at least one chip".into());
    }
    if args.tile_index != 0 {
        return Err("system execution runs every configured tile; tile-index must be zero".into());
    }
    let tile_count = config::tile_count();
    let harts: usize = (0..tile_count)
        .map(|tile| config::tile_topology(tile).cores.len() + 1)
        .sum();
    let chips: Vec<_> = (0..chip_count)
        .map(|_| Chip::new(args.memory_mib << 20, harts))
        .collect();
    for chip in &chips {
        chip.platform.lock().expect("BEMU platform poisoned").console_uart = false;
    }
    if let Some(io_chip) = vm_chip {
        if io_chip >= chip_count {
            return Err("VM chip is outside the system".into());
        }
        for chip in &chips {
            chip.capture_uart();
        }
    }
    let fabric = Fabric::connect(&chips);
    let mut machines = Vec::with_capacity(chip_count * tile_count);
    let mut endpoints = Vec::with_capacity(chip_count * tile_count);
    // Load all guests before enabling DMA, so ELF loading cannot erase an incoming transfer.
    for (id, chip) in chips.iter().enumerate() {
        let mut first_hart = 0;
        for tile in 0..tile_count {
            let mut local = args.clone();
            local.tile_index = tile;
            local.log_dir = args.log_dir.join(format!("chip-{id}/tile-{tile}"));
            std::fs::create_dir_all(&local.log_dir).map_err(|e| e.to_string())?;
            let machine = Machine::prepare(&local, chip, first_hart)?;
            endpoints.push(if host_io { Some(super::host_io::Endpoint::bind(&local.log_dir)?) } else { None });
            first_hart += config::tile_topology(tile).cores.len() + 1;
            machines.push((machine, local));
        }
    }
    let cancelled = AtomicBool::new(false);
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let vm_thread = vm_chip.map(|io_chip| {
            let (chips, directory, finished, cancelled, fabric, endpoints) =
                (&chips, &args.log_dir, &finished, &cancelled, &fabric, &endpoints);
            scope.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    super::vm::serve(chips, io_chip, directory, finished, cancelled)
                }))
                .unwrap_or_else(|_| Err("VM thread panicked".into()));
                if result.is_err() {
                    cancelled.store(true, Ordering::Release);
                    fabric.stop();
                    for endpoint in endpoints.iter().flatten() { endpoint.stop(); }
                }
                result
            })
        });
        let transfer_thread = scope.spawn(|| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fabric.serve(&chips)))
                .unwrap_or_else(|_| Err("interconnect worker panicked".into()));
            if result.is_err() {
                cancelled.store(true, Ordering::Release);
                for endpoint in endpoints.iter().flatten() { endpoint.stop(); }
            }
            result
        });
        let mut threads = Vec::new();
        for (index, (machine, local)) in machines.into_iter().enumerate() {
            let cancelled = &cancelled;
            let fabric = &fabric;
            let endpoints = &endpoints;
            threads.push(scope.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| machine.run(&local, cancelled, endpoints[index].as_ref())))
                    .unwrap_or_else(|_| Err("chip worker panicked".into()));
                if result.is_err() {
                    cancelled.store(true, Ordering::Release);
                    fabric.stop();
                    for endpoint in endpoints.iter().flatten() { endpoint.stop(); }
                }
                result
            }));
        }
        let mut error = None;
        for (id, thread) in threads.into_iter().enumerate() {
            if let Err(failure) = thread.join().map_err(|_| "chip thread panicked".to_string())? {
                error.get_or_insert(failure);
            } else {
                eprintln!("chip {} tile {}: program completed", id / tile_count, id % tile_count);
            }
        }
        fabric.stop();
        finished.store(true, Ordering::Release);
        if let Some(thread) = vm_thread {
            thread.join().map_err(|_| "VM thread panicked".to_string())??;
        }
        transfer_thread
            .join()
            .map_err(|_| "interconnect thread panicked".to_string())??;
        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    })
}
