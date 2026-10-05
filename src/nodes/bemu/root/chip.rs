use super::platform::Platform;
use std::sync::{Arc, Mutex};

pub struct Chip {
    pub(crate) clint: Arc<bebop_clint::Clint>,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) memory: Arc<super::memory::Ddr>,
}

impl Chip {
    pub fn push_live_microphone(&self, samples: Vec<i16>) -> Result<(), &'static str> {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.microphone.push_live(samples)
    }

    pub fn clock(&self) -> u64 {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.rtc.load(0, 8).unwrap()
    }

    pub fn set_clock(&self, ns: u64) {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        assert!(platform.rtc.store(0, 8, ns));
    }

    pub fn hart_count(&self) -> usize {
        self.platform.lock().expect("BEMU platform poisoned").exit_codes.len()
    }

    pub fn capture_uart(&self) {
        self.platform.lock().expect("BEMU platform poisoned").capture_uart = true;
    }

    pub fn take_uart(&self) -> Vec<(u32, Vec<u8>)> {
        self.platform.lock().expect("BEMU platform poisoned").take_uart()
    }
    pub fn push_microphone(&self, samples: impl IntoIterator<Item = i16>) {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.microphone.push(samples);
    }

    pub fn push_key(&self, code: u16, pressed: bool) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .keyboard
            .push(code, pressed);
    }

    pub fn take_audio(&self) -> Vec<i16> {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.speaker.take_samples()
    }

    pub fn take_frame(&self) -> Option<Vec<u8>> {
        self.platform.lock().expect("BEMU platform poisoned").vga.take_frame()
    }

    pub fn new(memory_size: usize, hart_count: usize) -> Self {
        let platform = Platform::new(memory_size, hart_count);
        Self {
            clint: Arc::clone(&platform.clint),
            memory: Arc::clone(&platform.memory),
            platform: Arc::new(Mutex::new(platform)),
        }
    }

    pub fn push_uart(&self, hart: usize, byte: u8) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .push_uart(hart, byte);
    }
}


pub(crate) fn run_system(args: &super::tile::run::Args) -> Result<(), String> {
    let chip = Chip::new(args.memory_mib << 20, crate::config::hart_capacity());
    let firmware = bebop_elf::analyze_elf(
        args.elf.to_str().ok_or("invalid firmware ELF path")?, super::platform::DRAM_BASE,
    )?;
    if firmware.os_abi != bebop_elf::OsAbi::Standalone {
        return Err("system firmware requires a standalone ELF".into());
    }
    let mut cores = Vec::new();
    let mut participants = Vec::new();
    for tile_index in 0..crate::config::tile_count() {
        let topology = crate::config::tile_topology(tile_index);
        let signatures = if topology.controller_core.is_some() {
            topology.worker_cores.iter().map(|(_, core)| crate::config::core_signature(*core)).collect()
        } else { Vec::new() };
        let tile = super::tile::Tile::new(&chip, &topology, signatures);
        for (_, core_index) in topology.cores {
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
    first.init_system(args.dtb.as_deref(), args.initrd.as_deref()).map_err(|error| error.to_string())?;
    let dtb = if args.dtb.is_some() { super::platform::DTB_ADDRESS } else { 0 };
    for core in cores.iter_mut().skip(1) { core.init_system_hart(firmware.entry, dtb); }
    println!("BEMU system: {} physical harts, shared DDR, embedded FDT={}", cores.len(), dtb == 0);
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
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
                        Some(code) => Err(format!("system firmware exited with code {code}")),
                    }
                })).unwrap_or_else(|_| Err("system hart thread panicked".to_string()));
                if result.is_err() { cancelled.store(true, std::sync::atomic::Ordering::Release); }
                result
            }));
        }
        let mut failure = None;
        for thread in threads {
            match thread.join() {
                Ok(Ok(())) => (),
                Ok(Err(error)) => { failure.get_or_insert(error); },
                Err(_) => { failure.get_or_insert("system hart thread panicked".to_string()); },
            }
        }
        match failure { Some(error) => Err(error), None => Ok(()) }
    })
}
