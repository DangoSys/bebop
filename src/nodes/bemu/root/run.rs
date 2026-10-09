use super::chip::Chip;
use clap::Parser;
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
pub struct Args {
    #[arg(long, default_value_t = 3072)]
    memory_mib: usize,
    #[arg(long)]
    elf: PathBuf,
    #[arg(long)]
    log_dir: PathBuf,
    #[arg(long)]
    dtb: Option<PathBuf>,
    #[arg(long)]
    initrd: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["dtb", "initrd", "memory_mib"])]
    load_manifest: Option<PathBuf>,
    #[arg(long)]
    disasm: bool,
    #[arg(long = "tool-profile")]
    profile: bool,
    #[arg(long)]
    itrace: bool,
    #[arg(long)]
    mtrace: bool,
}

pub fn run(args: Args) -> Result<(), String> {
    let plan = args.load_manifest.as_deref().map(|path|
        memory::load_manifest::validate_loads(None, Some(path))).transpose()?;
    let bytes = match &plan {
        Some(plan) => usize::try_from(plan.manifest.as_ref().unwrap().ddr_size)
            .map_err(|_| "manifest DDR size exceeds host address space")?,
        None => args.memory_mib << 20,
    };
    let chip = Chip::new(bytes, crate::config::hart_capacity());
    chip.capture_uart();
    let platform = Arc::clone(&chip.platform);
    let mut console_config = bebop_uart::ConsoleConfig::new("bemu");
    console_config.uart_log_dir = Some(args.log_dir.join("uart"));
    let console = bebop_uart::ConsoleServer::start(&args.log_dir, console_config, move |hart, byte| {
        platform.lock().expect("BEMU platform poisoned").push_uart(hart as usize, byte);
    })?;
    println!("Console socket: {}", console.socket_path().display());
    let firmware = bebop_elf::analyze_elf(
        args.elf.to_str().ok_or("invalid firmware ELF path")?, super::platform::DRAM_BASE,
    )?;
    if firmware.os_abi != bebop_elf::OsAbi::Standalone {
        return Err("chip firmware requires a standalone ELF".into());
    }
    if let Some(plan) = &plan {
        let manifest = plan.manifest.as_ref().unwrap();
        let boot = &plan.loads[0];
        if firmware.is_pie || firmware.needs_relocation || firmware.entry != manifest.ddr_base {
            return Err("manifest firmware requires a fixed ELF entry at DDR base".into());
        }
        for segment in &firmware.load_segments {
            let offset = segment.vaddr.checked_sub(manifest.ddr_base)
                .ok_or("ELF load segment precedes DDR")?;
            if (segment.filesz != 0 && offset.checked_add(segment.filesz).ok_or("ELF file range overflows")? > boot.size)
                || offset.checked_add(segment.memsz).ok_or("ELF memory range overflows")?
                    > manifest.fdt_base - manifest.ddr_base
            {
                return Err("ELF load segments exceed manifest boot or guest memory region".into());
            }
        }
    }
    let mut cores = Vec::new();
    let mut participants = Vec::new();
    for tile_index in 0..crate::config::tile_count() {
        let topology = crate::config::tile_topology(tile_index);
        let signatures = if topology.controller_core.is_some() {
            topology.worker_cores.iter().map(|(_, core)| crate::config::core_signature(*core)).collect()
        } else { Vec::new() };
        let tile = super::tile::Tile::new(&chip, &topology, signatures);
        for (_, core_index) in topology.cores.into_iter().filter(|(_,core)| crate::config::ant_config(*core).is_none()) {
            let hart = crate::config::core_hart_id(core_index);
            participants.push(hart);
            cores.push(crate::Core::new_with_core_hart(
                &args.log_dir.join(format!("hart-{hart}")),
                crate::TraceConfig::new(args.itrace, args.mtrace), args.disasm, args.profile,
                core_index, hart, Some(Arc::clone(&tile)),
            ).map_err(|error| error.to_string())?);
        }
    }
    chip.clint.coordinate(participants);
    let first = cores.first_mut().ok_or("chip has no harts")?;
    first.load_elf(&args.elf).map_err(|error| error.to_string())?;
    if let Some(plan) = &plan {
        use std::io::Read;
        let manifest = plan.manifest.as_ref().unwrap();
        let mut file_bytes = [0u8; 65536];
        let mut loaded_bytes = [0u8; 65536];
        for entry in &plan.loads {
            let mut file = std::fs::File::open(&entry.file).map_err(|error| error.to_string())?;
            let mut offset = 0;
            while offset < entry.size {
                let count = (entry.size - offset).min(file_bytes.len() as u64) as usize;
                file.read_exact(&mut file_bytes[..count]).map_err(|error|
                    format!("read {} at {offset}: {error}", entry.file.display()))?;
                let address = manifest.ddr_base + entry.offset + offset;
                if entry.role == "boot" {
                    chip.memory.read_buffer(address, &mut loaded_bytes[..count])
                        .map_err(|_| format!("boot comparison outside DDR at 0x{address:x}"))?;
                    if loaded_bytes[..count] != file_bytes[..count] {
                        let byte = loaded_bytes[..count].iter().zip(&file_bytes[..count])
                            .position(|(loaded, expected)| loaded != expected).unwrap();
                        return Err(format!("ELF differs from manifest boot image at physical address 0x{:x}", address + byte as u64));
                    }
                } else {
                    chip.memory.write_buffer(address, &file_bytes[..count])
                        .map_err(|_| format!("model load outside DDR at 0x{address:x}"))?;
                }
                offset += count as u64;
            }
            println!("BEMU {} {}: {} bytes at 0x{:x}",
                if entry.role == "boot" { "verified" } else { "loaded" }, entry.role,
                entry.size, manifest.ddr_base + entry.offset);
        }
    }
    first.init_firmware(args.dtb.as_deref(), args.initrd.as_deref()).map_err(|error| error.to_string())?;
    let dtb = if args.dtb.is_some() { super::platform::DTB_ADDRESS } else { 0 };
    for core in cores.iter_mut().skip(1) { core.init_firmware_hart(firmware.entry, dtb); }
    println!("BEMU chip: {} physical harts, shared DDR, embedded FDT={}", cores.len(), dtb == 0);
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let finished = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let uart = scope.spawn(|| {
            loop {
                let done = finished.load(std::sync::atomic::Ordering::Acquire);
                for (hart, bytes) in chip.take_uart() {
                    for byte in bytes { console.send_tx(hart, byte); }
                }
                if done { break; }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });
        let mut threads = Vec::new();
        for mut core in cores {
            let cancelled = &cancelled;
            threads.push(scope.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    while !core.finished() && !cancelled.load(std::sync::atomic::Ordering::Acquire) {
                        core.step(256).map_err(|error| error.to_string())?;
                        std::thread::yield_now();
                    }
                    match core.exit_code() {
                        Some(0) | None => Ok(()),
                        Some(code) => Err(format!("chip firmware exited with code {code}")),
                    }
                })).unwrap_or_else(|_| Err("chip hart thread panicked".to_string()));
                if result.is_err() { cancelled.store(true, std::sync::atomic::Ordering::Release); }
                result
            }));
        }
        let mut failure = None;
        for thread in threads {
            match thread.join() {
                Ok(Ok(())) => (),
                Ok(Err(error)) => { failure.get_or_insert(error); },
                Err(_) => { failure.get_or_insert("chip hart thread panicked".to_string()); },
            }
        }
        finished.store(true, std::sync::atomic::Ordering::Release);
        uart.join().map_err(|_| "chip UART thread panicked".to_string())?;
        match failure { Some(error) => Err(error), None => Ok(()) }
    })
}
