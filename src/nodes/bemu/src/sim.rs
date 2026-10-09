use crate::{
    accel, config,
    process::{write_guest, Process},
    root::{
        chip::Chip,
        mmu::GuestAccess,
        platform::{Port, DRAM_BASE, DTB_ADDRESS, INITRD_ADDRESS},
        tile::{tasks::Workers, Tile},
    },
    trace::TraceConfig,
};
use bebop_bemu_profile::BemuProfileReport;
use bebop_elf::{analyze_elf, load_elf, LoadInfo};
use rvsim::{
    hart::{Hart, Step},
    mmu::Mmu,
};
use snafu::{FromString, OptionExt, ResultExt, Whatever};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

pub struct Core {
    hart: Hart,
    mmu: Mmu,
    tile: Arc<Tile>,
    accel: accel::State,
    workers: Option<Workers>,
    process: Option<Process>,
    loaded_elf: Option<LoadInfo>,
    image_physical: (u64, u64),
    core_index: usize,
    is_controller: bool,
    disasm: Option<BufWriter<File>>,
    profile: bool,
    cpu_elapsed: Duration,
    elapsed_cycles: u64,
    exit_code: Option<i32>,
    waiting: bool,
}

impl Core {
    pub fn hart_id(&self) -> usize { self.hart.id as usize }
    pub fn chip(&self) -> Chip {
        Chip {
            tiles: Arc::clone(&self.tile.tile_registry),
            clint: Arc::clone(&self.tile.clint),
            platform: Arc::clone(&self.tile.platform),
            memory: Arc::clone(&self.tile.memory),
        }
    }

    pub fn new(log_dir: &Path, trace: TraceConfig, disasm: bool, profile: bool) -> Result<Self, Whatever> {
        Self::new_with_core(log_dir, trace, disasm, profile, 0)
    }

    pub fn new_with_core(
        log_dir: &Path,
        trace: TraceConfig,
        disasm: bool,
        profile: bool,
        core_index: usize,
    ) -> Result<Self, Whatever> {
        Self::new_with_core_hart(log_dir, trace, disasm, profile, core_index, config::core_hart_id(core_index), None)
    }

    pub fn new_with_core_hart(
        log_dir: &Path,
        trace: TraceConfig,
        disasm: bool,
        profile: bool,
        core_index: usize,
        hart_id: usize,
        shared: Option<Arc<Tile>>,
    ) -> Result<Self, Whatever> {
        config::configure_core(core_index);
        std::fs::create_dir_all(log_dir).whatever_context("create BEMU log directory")?;
        assert_eq!(hart_id, config::core_hart_id(core_index), "hart ID differs from PB core placement");
        let tile = shared.unwrap_or_else(|| {
            let topology = config::tile_for_core(core_index);
            let signatures = if topology.controller_core.is_some() {
                topology.worker_cores.iter().map(|(_, core)| config::core_signature(*core)).collect()
            } else { Vec::new() };
            Tile::new(&Chip::new(3 * (1 << 30), config::hart_capacity()), &topology, signatures)
        });
        assert!(tile.harts.contains(&hart_id), "core does not belong to Tile");
        let endpoint_index = tile.endpoint_ids.iter().position(|&id| id == core_index);
        let is_controller = tile.controller_id == Some(core_index);
        let accel = accel::State::new(log_dir, trace.clone(), profile, core_index,
            endpoint_index, Some(Arc::clone(&tile))).map_err(Whatever::without_source)?;
        let workers = if is_controller {
            Some(tile.tasks.start(&tile, log_dir, trace, profile).map_err(Whatever::without_source)?)
        } else { None };
        config::configure_core(core_index);
        tile.platform.lock().expect("BEMU platform poisoned").uart_log(
            hart_id,
            File::create(log_dir.join("uart.log")).whatever_context("create UART log")?,
            bebop_uart::CycleTraceCollector::new(log_dir).map_err(Whatever::without_source)?,
        );
        let disasm = if disasm {
            Some(BufWriter::new(
                File::create(log_dir.join("disasm.log")).whatever_context("create instruction log")?,
            ))
        } else {
            None
        };
        let hart = Hart::new(hart_id as u64, DRAM_BASE);
        Ok(Self {
            hart,
            mmu: Mmu::default(),
            tile,
            accel,
            workers,
            process: Some(Process::new()),
            loaded_elf: None,
            image_physical: (DRAM_BASE, 0),
            core_index,
            is_controller,
            disasm,
            profile,
            cpu_elapsed: Duration::ZERO,
            elapsed_cycles: 0,
            exit_code: None,
            waiting: false,
        })
    }

    pub fn load_elf(&mut self, elf: &Path) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        let path = elf.to_str().whatever_context("invalid ELF path")?;
        self.process
            .as_mut()
            .expect("process must exist before loading firmware")
            .program_name = path.to_owned();
        let platform = self.tile.platform.lock().expect("BEMU platform poisoned");
        let analysis = analyze_elf(path, DRAM_BASE).map_err(Whatever::without_source)?;
        if analysis.os_abi == bebop_elf::OsAbi::GnuUser {
            let bytes = crate::process::align_up(analysis.image_end - DRAM_BASE, crate::process::PAGE_SIZE);
            let address = platform
                .pages
                .lock()
                .expect("DDR page pool poisoned")
                .allocate(bytes)
                .map_err(Whatever::without_source)?;
            let offset = (address - DRAM_BASE) as usize;
            self.image_physical = (address, bytes);
            let memory = self.tile.memory.as_ref();
            memory.fill(address, bytes as usize, 0).expect("ELF image range");
            self.loaded_elf = Some(load_elf(path, memory, DRAM_BASE, offset).map_err(Whatever::without_source)?);
        } else {
            self.loaded_elf =
                Some(load_elf(path, self.tile.memory.as_ref(), DRAM_BASE, 0).map_err(Whatever::without_source)?);
        }
        self.process
            .as_mut()
            .expect("ELF must be loaded before firmware initialization")
            .syscall
            .working_dir = elf.parent().expect("ELF parent directory").to_path_buf();
        Ok(())
    }

    pub fn set_working_directory(&mut self, directory: PathBuf) {
        self.process
            .as_mut()
            .expect("guest working directory requires process mode")
            .syscall
            .working_dir = directory;
    }

    pub fn set_arguments(&mut self, arguments: Vec<String>) {
        self.process
            .as_mut()
            .expect("guest arguments require process mode")
            .arguments = arguments;
    }

    pub fn init_hart(&mut self) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load ELF before initializing the hart")?;
        let platform = self.tile.platform.lock().expect("BEMU platform poisoned");
        let pages = Arc::clone(&platform.pages);
        self.process
            .as_mut()
            .expect("process mode is unavailable")
            .initialize(
                &mut self.hart,
                self.tile.memory.as_ref(),
                load,
                pages,
                self.image_physical,
            )
            .map_err(Whatever::without_source)
    }

    pub fn init_firmware(&mut self, dtb: Option<&Path>, initrd: Option<&Path>) -> Result<(), Whatever> {
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load firmware ELF before initializing firmware")?;
        if let Some(path) = dtb {
            let bytes = std::fs::read(path).whatever_context("read firmware DTB")?;
            write_guest(self.tile.memory.as_ref(), DTB_ADDRESS, &bytes).map_err(Whatever::without_source)?;
        }
        if let Some(path) = initrd {
            let image = std::fs::read(path).whatever_context("read initramfs")?;
            write_guest(self.tile.memory.as_ref(), INITRD_ADDRESS, &image).map_err(Whatever::without_source)?;
        }
        self.init_firmware_hart(load.entry, if dtb.is_some() { DTB_ADDRESS } else { 0 });
        Ok(())
    }

    pub(crate) fn init_firmware_hart(&mut self, entry: u64, dtb: u64) {
        self.process = None;
        self.hart.privilege = rvsim::Privilege::Machine;
        self.hart.pc = entry;
        self.hart.set_register(10, self.hart.id);
        self.hart.set_register(11, dtb);
    }

    fn control(&mut self, operation:u32, a:u64, b:u64)->Result<u64,Whatever> {
        if (13..=19).contains(&operation) {
            assert!(self.is_controller || (self.tile.tile_index == 0 && self.core_index == self.tile.execution_ids[0]),
                "shared storage management requires the tile control CPU");
            Ok(self.tile.t2t_control(operation,a,b))
        } else {
            assert!(self.is_controller,"Ant management requires tile controller");
            Ok(self.tile.tasks.control(operation,a,b,&self.hart))
        }
    }

    pub fn step(&mut self, count: u64) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        for _ in 0..count {
            let code = self.tile.exit_code.load(std::sync::atomic::Ordering::Acquire);
            if code != i64::MIN {
                self.exit_code = Some(code as i32);
            }
            if self.exit_code.is_some() {
                break;
            }
            let next_cycle = if self.waiting {
                self.elapsed_cycles.max(self.tile.clint.cycles() + 10_000)
            } else {
                self.elapsed_cycles + 1
            };
            let cycles = next_cycle - self.elapsed_cycles;
            self.elapsed_cycles = next_cycle;
            self.tile.clint.advance_to(self.hart.id as usize, self.elapsed_cycles);
            let inputs = crate::input::Inputs {
                cycles,
                hart: self.hart.id as usize,
                clint: &self.tile.clint,
                snapshot: std::cell::Cell::new(None),
            };
            let pc = self.hart.pc;
            let privilege = self.hart.privilege;
            if let Some(workers) = &self.workers {
                workers.check().map_err(Whatever::without_source)?;
            }
            let started = self.profile.then(Instant::now);
            let mut event = self.hart.step(
                &mut Port { devices: &self.tile.platform, memory: &self.tile.memory },
                &self.mmu, inputs,
            );
            if let Some(started) = started {
                self.cpu_elapsed += started.elapsed();
            }
            self.waiting = event == Step::Waiting;
            if let Step::Custom(request) = event {
                let value = match request.instruction & 0x7f {
                    0x7b => {
                        let mut translation = self.hart.translation_context();
                        if translation.privilege == rvsim::Privilege::Machine
                            && translation.satp != rvsim::mmu::Satp::Bare
                        {
                            translation.privilege = rvsim::Privilege::Supervisor;
                        }
                        let guest = GuestAccess::new(Port {
                            devices: &self.tile.platform, memory: &self.tile.memory,
                        }, &self.mmu, translation);
                        accel::execute(
                            &mut self.accel,
                            guest,
                            (request.instruction >> 25) as u8,
                            request.rs1,
                            request.rs2,
                            request.pc,
                        )
                    }
                    0x2b => {
                        let operation = request.instruction >> 25;
                        if (13..=19).contains(&operation) {
                            assert_eq!((request.instruction >> 12) & 7, 7, "T2T management requires funct3=7");
                        }
                        Ok(self.control(operation, request.rs1, request.rs2)?)
                    },
                    _ => {
                        return Err(Whatever::without_source(format!(
                            "unsupported custom opcode: 0x{:08x}",
                            request.instruction
                        )))
                    }
                };
                event = self
                    .hart
                    .complete_custom(value.map(|value| (request.instruction & (1 << 14) != 0).then_some(value)));
            }
            match event {
                Step::Retired { pc, instruction } => {
                    if let Some(log) = &mut self.disasm {
                        let decoded = if instruction & 3 == 3 {
                            instruction
                        } else {
                            rvsim::expand_compressed(instruction as u16).expect("retired compressed instruction")
                        };
                        writeln!(
                            log,
                            "hart={} privilege={privilege:?} pc=0x{pc:016x} inst=0x{instruction:08x} {}",
                            self.hart.id,
                            bebop_dasm::disassemble(decoded)
                        )
                        .whatever_context("write instruction log")?;
                    }
                }
                Step::Trap(trap) => {
                    if let Some(log) = &mut self.disasm {
                        writeln!(
                            log,
                            "hart={} pc=0x{pc:016x} trap={} value=0x{:x}",
                            self.hart.id, trap.cause, trap.value
                        )
                        .whatever_context("write trap log")?;
                    }
                    if let Some(process) = &mut self.process {
                        if trap.cause == if process.user_mode { 8 } else { 11 } {
                            process
                                .syscall(&mut self.hart, &self.tile.platform)
                                .map_err(Whatever::without_source)?;
                            self.tile.memory.clear_reservations();
                            if self.hart.register(17) == 124 {
                                std::thread::yield_now();
                            }
                            if matches!(self.hart.register(17), 214 | 215 | 222) {
                                self.mmu.fence();
                            }
                            self.exit_code = process.syscall.exit_code;
                        } else if process.user_mode {
                            return Err(Whatever::without_source(format!(
                                "user trap at pc=0x{pc:x}: cause={} value=0x{:x}",
                                trap.cause, trap.value
                            )));
                        }
                    }
                }
                Step::Waiting => break,
                Step::Custom(_) => unreachable!(),
            }
            if self.accel.barrier_hit {
                break;
            }
        }
        if self.exit_code.is_some() {
            if let Some(workers) = &mut self.workers {
                workers.shutdown().map_err(Whatever::without_source)?;
            }
            self.tile
                .platform
                .lock()
                .expect("BEMU platform poisoned")
                .flush_uart(self.hart.id as usize)
                .whatever_context("flush UART log")?;
            if let Some(log) = &mut self.disasm {
                log.flush().whatever_context("flush instruction log")?;
            }
        } else if self.waiting {
            std::thread::sleep(Duration::from_micros(50));
        }
        Ok(())
    }

    pub fn take_barrier(&mut self) -> bool {
        std::mem::take(&mut self.accel.barrier_hit)
    }
    pub fn stop(&mut self, code: i32) {
        self.exit_code = Some(code);
    }
    pub fn finished(&self) -> bool {
        self.exit_code.is_some()
    }
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
    pub fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        self.accel.profile.report(total, self.cpu_elapsed)
    }
}
