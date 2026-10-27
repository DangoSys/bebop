use crate::DeviceModel;
use clap::Parser;
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
pub struct Args {
    #[arg(long)]
    elf: PathBuf,
    #[arg(long)]
    pub log_dir: PathBuf,
    #[arg(long)]
    dtb: Option<PathBuf>,
    #[arg(long)]
    initrd: Option<PathBuf>,
    /// The DDR image (the kernel and the model, from `bbdev kernel --build`),
    /// loaded at the start of the DDR. The ELF still gives the entry.
    #[arg(long)]
    ddr_image: Option<PathBuf>,
    #[arg(long)]
    pub disasm: bool,
    #[arg(long = "tool-profile")]
    pub profile: bool,
    #[arg(long)]
    pub itrace: bool,
    #[arg(long)]
    pub mtrace: bool,
    #[arg(long)]
    pub ctrace: bool,
    #[arg(long)]
    pub banktrace: bool,
}

impl Args {
    pub fn model_options(&self) -> crate::ModelOptions {
        let mut trace = crate::TraceConfig::new(self.itrace, self.mtrace);
        trace.ctrace = self.ctrace;
        trace.btrace = self.banktrace;
        crate::ModelOptions {
            log_dir: self.log_dir.clone(),
            trace,
            disasm: self.disasm,
            profile: self.profile,
        }
    }
}

/// What a simulation entry gives the runner: its design.pb and how its device
/// is instantiated.
pub struct Simulation {
    pub design: &'static [u8],
    pub instantiate: fn(&DeviceModel, &Args) -> Result<Vec<Box<dyn crate::CoreModel>>, String>,
}

pub fn run(args: Args, simulation: &Simulation) -> Result<(), String> {
    crate::configs::load(simulation.design);
    let (ddr_base, ddr_bytes, _) = crate::configs::ddr();
    let design = DeviceModel::new(ddr_bytes as usize, crate::configs::hart_capacity());
    if let Some(image) = &args.ddr_image {
        let mut file = std::fs::File::open(image).map_err(|error| format!("DDR image {}: {error}", image.display()))?;
        let mut chunk = vec![0u8; 1 << 20];
        let mut address = ddr_base;
        loop {
            let count = std::io::Read::read(&mut file, &mut chunk).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            // The image is sparse: do not allocate the pages that are zero.
            if chunk[..count].iter().any(|byte| *byte != 0) {
                design
                    .memory
                    .write_buffer(address, &chunk[..count])
                    .map_err(|_| "DDR image outside the DDR".to_string())?;
            }
            address += count as u64;
        }
    }
    design.capture_uart();
    let platform = Arc::clone(&design.platform);
    let mut console_config = bebop_uart::ConsoleConfig::new("bemu");
    console_config.uart_log_dir = Some(args.log_dir.join("uart"));
    let console = bebop_uart::ConsoleServer::start(&args.log_dir, console_config, move |hart, byte| {
        platform
            .lock()
            .expect("BEMU platform poisoned")
            .push_uart(hart as usize, byte);
    })?;
    println!("Console socket: {}", console.socket_path().display());
    let firmware = bebop_elf::analyze_elf(
        args.elf.to_str().ok_or("invalid firmware ELF path")?,
        crate::device::platform::DRAM_BASE,
    )?;
    if firmware.os_abi != bebop_elf::OsAbi::Standalone {
        return Err("design firmware requires a standalone ELF".into());
    }
    let mut threads = (simulation.instantiate)(&design, &args)?;
    let participants = threads.iter().map(|core| core.cpu().hart_id()).collect();
    design.clint.coordinate(participants);
    let first = threads.first_mut().ok_or("design has no harts")?;
    first.cpu_mut().load_elf(&args.elf).map_err(|error| error.to_string())?;
    first
        .cpu_mut()
        .init_firmware(args.dtb.as_deref(), args.initrd.as_deref())
        .map_err(|error| error.to_string())?;
    let dtb = if args.dtb.is_some() {
        crate::device::platform::DTB_ADDRESS
    } else {
        0
    };
    for thread in threads.iter_mut().skip(1) {
        thread.cpu_mut().init_firmware_hart(firmware.entry, dtb);
    }
    println!(
        "BEMU design: {} physical harts, shared DDR, embedded FDT={}",
        threads.len(),
        dtb == 0
    );
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let finished = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let uart = scope.spawn(|| loop {
            let done = finished.load(std::sync::atomic::Ordering::Acquire);
            for (hart, bytes) in design.take_uart() {
                for byte in bytes {
                    console.send_tx(hart, byte);
                }
            }
            if done {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        });
        let mut workers = Vec::new();
        for mut thread in threads {
            let cancelled = &cancelled;
            workers.push(scope.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    while !thread.cpu().finished() && !cancelled.load(std::sync::atomic::Ordering::Acquire) {
                        thread.step(256).map_err(|error| error.to_string())?;
                        std::thread::yield_now();
                    }
                    match thread.cpu().exit_code() {
                        Some(0) | None => Ok(()),
                        Some(code) => Err(format!("design firmware exited with code {code}")),
                    }
                }))
                .unwrap_or_else(|_| Err("design hart thread panicked".to_string()));
                if result.is_err() {
                    cancelled.store(true, std::sync::atomic::Ordering::Release);
                }
                result
            }));
        }
        let mut failure = None;
        for worker in workers {
            match worker.join() {
                Ok(Ok(())) => (),
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert("design hart thread panicked".to_string());
                }
            }
        }
        finished.store(true, std::sync::atomic::Ordering::Release);
        uart.join().map_err(|_| "design UART thread panicked".to_string())?;
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    })
}
