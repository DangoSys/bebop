use super::{CoreState, GuestAccess, Rocc};
use crate::core::riscv::{
    hart::{Hart, Step},
    mmu::Mmu,
};
use crate::{
    configs,
    device::{
        platform::{Port, DRAM_BASE, DTB_ADDRESS, INITRD_ADDRESS},
        DeviceModel,
    },
    sim::process::{write_guest, Process},
};
use bebop_bemu_profile::{BemuProfile, BemuProfileReport};
use bebop_elf::{analyze_elf, load_elf, LoadInfo};
use snafu::{FromString, OptionExt, ResultExt, Whatever};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// A core's RV64 hart: it runs the program and hands each custom-3 instruction
/// to what the core connects to its RoCC port.
pub struct Cpu {
    hart: Hart,
    mmu: Mmu,
    core: Arc<CoreState>,
    process: Option<Process>,
    loaded_elf: Option<LoadInfo>,
    image_physical: (u64, u64),
    hart_id: usize,
    disasm: Option<BufWriter<File>>,
    profile: bool,
    cpu_elapsed: Duration,
    elapsed_cycles: u64,
    timing: super::timing::Timing,
    instruction_cycles: u64,
    next_timer_tick: u64,
    parallel: bool,
    throttled: bool,
    exit_code: Option<i32>,
    waiting: bool,
}

impl Cpu {
    pub fn hart_id(&self) -> usize {
        self.hart.id as usize
    }
    pub fn new(
        device: &DeviceModel,
        params: &crate::CoreParams,
        options: &crate::ModelOptions,
    ) -> Result<Self, Whatever> {
        let log_dir = &options.log_dir;
        let disasm = options.disasm;
        let profile = options.profile;
        let endpoint = configs::Endpoint::Core(params.hart_id);
        configs::configure(endpoint);
        std::fs::create_dir_all(log_dir).whatever_context("create core log directory")?;
        let core = CoreState::new(device, params);
        let hart_id = params.hart_id;
        let hart = Hart::new(hart_id as u64, DRAM_BASE);
        let mmu = Mmu::default();
        core.platform.lock().expect("BEMU platform poisoned").uart_log(
            hart_id,
            File::create(log_dir.join("uart.log")).whatever_context("create UART log")?,
        );
        let disasm = if disasm {
            Some(BufWriter::new(
                File::create(log_dir.join("disasm.log")).whatever_context("create instruction log")?,
            ))
        } else {
            None
        };
        Ok(Self {
            hart,
            mmu,
            core,
            process: Some(Process::new()),
            loaded_elf: None,
            image_physical: (DRAM_BASE, 0),
            hart_id,
            disasm,
            profile,
            cpu_elapsed: Duration::ZERO,
            elapsed_cycles: 0,
            timing: super::timing::Timing::new(params.pipeline),
            instruction_cycles: 1,
            next_timer_tick: crate::device::clint::TICK_CYCLES,
            parallel: device.hart_count() > 1,
            throttled: false,
            exit_code: None,
            waiting: false,
        })
    }

    pub fn load_elf(&mut self, elf: &Path) -> Result<(), Whatever> {
        configs::configure(configs::Endpoint::Core(self.hart_id));
        let path = elf.to_str().whatever_context("invalid ELF path")?;
        self.process
            .as_mut()
            .expect("process must exist before loading firmware")
            .program_name = path.to_owned();
        let platform = self.core.platform.lock().expect("BEMU platform poisoned");
        let analysis = analyze_elf(path, DRAM_BASE).map_err(Whatever::without_source)?;
        if analysis.os_abi == bebop_elf::OsAbi::GnuUser {
            let bytes = crate::sim::process::align_up(analysis.image_end - DRAM_BASE, crate::sim::process::PAGE_SIZE);
            let address = platform
                .pages
                .lock()
                .expect("DDR page pool poisoned")
                .allocate(bytes)
                .map_err(Whatever::without_source)?;
            let offset = (address - DRAM_BASE) as usize;
            self.image_physical = (address, bytes);
            let memory = self.core.memory.as_ref();
            memory.fill(address, bytes as usize, 0).expect("ELF image range");
            self.loaded_elf = Some(load_elf(path, memory, DRAM_BASE, offset).map_err(Whatever::without_source)?);
        } else {
            self.loaded_elf =
                Some(load_elf(path, self.core.memory.as_ref(), DRAM_BASE, 0).map_err(Whatever::without_source)?);
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
        configs::configure(configs::Endpoint::Core(self.hart_id));
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load ELF before initializing the hart")?;
        let platform = self.core.platform.lock().expect("BEMU platform poisoned");
        let pages = Arc::clone(&platform.pages);
        self.process
            .as_mut()
            .expect("process mode is unavailable")
            .initialize(
                &mut self.hart,
                self.core.memory.as_ref(),
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
            write_guest(self.core.memory.as_ref(), DTB_ADDRESS, &bytes).map_err(Whatever::without_source)?;
        }
        if let Some(path) = initrd {
            let image = std::fs::read(path).whatever_context("read initramfs")?;
            write_guest(self.core.memory.as_ref(), INITRD_ADDRESS, &image).map_err(Whatever::without_source)?;
        }
        self.init_firmware_hart(load.entry, if dtb.is_some() { DTB_ADDRESS } else { 0 });
        Ok(())
    }

    pub fn init_firmware_hart(&mut self, entry: u64, dtb: u64) {
        self.process = None;
        self.hart.privilege = crate::core::riscv::Privilege::Machine;
        self.hart.pc = entry;
        self.hart.set_register(10, self.hart.id);
        self.hart.set_register(11, dtb);
    }

    fn control(&mut self, operation: u32, a: u64, b: u64) -> Result<u64, Whatever> {
        if (20..=23).contains(&operation) {
            return Ok(self
                .core
                .thread_memory
                .get()
                .expect("Core has no dataflow service")
                .control(operation, a, b));
        }
        if operation == 2 {
            let thread = (a >> 32) as usize;
            let task = self.core.tasks.contexts[thread].control.lock().unwrap().descriptor[0];
            if let Some(memory) = self.core.thread_memory.get() {
                memory.start(thread, task);
            }
        }
        if (13..=19).contains(&operation) {
            Ok(self.core.service.as_ref().expect("Core has no device service").request(
                self.core.hart_id,
                operation,
                a,
                b,
            ))
        } else {
            let before = self.elapsed_cycles;
            let value = self
                .core
                .tasks
                .control(operation, a, b, &self.hart, &mut self.elapsed_cycles);
            if self.hart.csrs.inhibit & 1 == 0 {
                self.hart.csrs.cycle = self.hart.csrs.cycle.wrapping_add(self.elapsed_cycles - before);
            }
            self.core.clint.advance_to(self.hart_id, self.elapsed_cycles);
            Ok(value)
        }
    }

    /// The state the core shares with what is connected to it.
    pub fn state(&self) -> &Arc<CoreState> {
        &self.core
    }

    pub fn step(&mut self, count: u64, rocc: &mut dyn Rocc) -> Result<(), Whatever> {
        configs::configure(configs::Endpoint::Core(self.hart_id));
        self.throttled = false;
        // Bound temporal skew without synchronizing host threads per instruction.
        let deadline = if self.parallel {
            self.core.clint.cycles() + 4096
        } else {
            u64::MAX
        };
        for _ in 0..count {
            if !self.waiting && self.elapsed_cycles >= deadline {
                self.throttled = true;
                break;
            }
            let code = self.core.exit_code.load(std::sync::atomic::Ordering::Acquire);
            if code != i64::MIN {
                self.core.clint.halt();
                self.exit_code = Some(code as i32);
            }
            if self.exit_code.is_some() {
                break;
            }
            let next_cycle = if self.waiting {
                self.elapsed_cycles.max(self.core.clint.cycles())
            } else {
                self.elapsed_cycles + self.instruction_cycles
            };
            let cycles = next_cycle - self.elapsed_cycles;
            self.elapsed_cycles = next_cycle;
            // Publish at timer boundaries and scheduling events, not at every instruction.
            if self.elapsed_cycles >= self.next_timer_tick {
                let tick = crate::device::clint::TICK_CYCLES;
                self.next_timer_tick = (self.elapsed_cycles / tick + 1) * tick;
                self.core.clint.advance_to(self.hart_id, self.elapsed_cycles);
            }
            let inputs = crate::sim::input::Inputs {
                cycles,
                hart: self.hart.id as usize,
                clint: &self.core.clint,
                snapshot: std::cell::Cell::new(None),
            };
            let pc = self.hart.pc;
            let privilege = self.hart.privilege;
            let started = self.profile.then(Instant::now);
            let mut event = self.hart.step(
                &mut Port {
                    cache: Some(&self.core.cache),
                    devices: &self.core.platform,
                    memory: &self.core.memory,
                },
                &self.mmu,
                inputs,
            );
            if let Some(started) = started {
                self.cpu_elapsed += started.elapsed();
            }
            let waiting = event == Step::Waiting;
            if waiting != self.waiting {
                self.core.clint.advance_to(self.hart_id, self.elapsed_cycles);
                self.core.clint.set_waiting(self.hart_id, waiting, self.hart.csrs.ie);
                self.waiting = waiting;
            }
            if let Step::Custom(request) = event {
                self.core.clint.advance_to(self.hart_id, self.elapsed_cycles);
                let value = match request.instruction & 0x7f {
                    0x7b => {
                        let translation = self.hart.translation_context();
                        let guest = GuestAccess::new(
                            Port {
                                cache: None,
                                devices: &self.core.platform,
                                memory: &self.core.memory,
                            },
                            &self.mmu,
                            translation,
                            &self.core.transfers,
                        );
                        rocc.custom(guest, request, self.elapsed_cycles)
                    }
                    0x2b => {
                        let operation = request.instruction >> 25;
                        if (13..=19).contains(&operation) {
                            assert_eq!(
                                (request.instruction >> 12) & 7,
                                7,
                                "Device management requires funct3=7"
                            );
                        }
                        Ok(self.control(operation, request.rs1, request.rs2)?)
                    }
                    _ => Err(crate::core::riscv::Trap::illegal(request.instruction)),
                };
                event = self
                    .hart
                    .complete_custom(value.map(|value| (request.instruction & (1 << 14) != 0).then_some(value)));
            }
            match event {
                Step::Retired { pc, instruction } => {
                    self.instruction_cycles = self.timing.instruction(pc, instruction);
                    if let Some(log) = &mut self.disasm {
                        let decoded = if instruction & 3 == 3 {
                            instruction
                        } else {
                            crate::core::riscv::expand_compressed(instruction as u16)
                                .expect("retired compressed instruction")
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
                                .syscall(&mut self.hart, &self.core.platform)
                                .map_err(Whatever::without_source)?;
                            self.core.memory.clear_reservations();
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
            if rocc.barrier() {
                break;
            }
        }
        self.core.clint.advance_to(self.hart_id, self.elapsed_cycles);
        if self.exit_code.is_some() {
            self.core.clint.finish(self.hart_id);
            self.core
                .platform
                .lock()
                .expect("BEMU platform poisoned")
                .flush_uart(self.hart.id as usize)
                .whatever_context("flush UART log")?;
            if let Some(log) = &mut self.disasm {
                log.flush().whatever_context("flush instruction log")?;
            }
        }
        Ok(())
    }

    pub(crate) fn wait_for_event(&mut self) {
        if self.throttled {
            self.core.clint.wait_for_progress(self.elapsed_cycles - 4096);
        }
        if !self.waiting {
            return;
        }
        self.core.clint.advance_to(self.hart_id, self.core.clint.cycles());
        self.core.clint.wait(self.hart_id, self.hart.csrs.ie);
        let now = self.elapsed_cycles.max(self.core.clint.cycles());
        if self.hart.csrs.inhibit & 1 == 0 {
            self.hart.csrs.cycle = self.hart.csrs.cycle.wrapping_add(now - self.elapsed_cycles);
        }
        self.elapsed_cycles = now;
        self.core.clint.advance_to(self.hart_id, now);
        self.waiting = false;
        self.instruction_cycles = 1;
        self.core.clint.set_waiting(self.hart_id, false, self.hart.csrs.ie);
    }

    pub fn stop(&mut self, code: i32) {
        self.exit_code = Some(code);
        self.core.clint.finish(self.hart_id);
    }
    pub fn finished(&self) -> bool {
        self.exit_code.is_some()
    }
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
    pub fn cpu_elapsed(&self) -> Duration {
        self.cpu_elapsed
    }
    pub fn counters(&self) -> (u64, u64) {
        (self.hart.csrs.instret, self.elapsed_cycles)
    }
    pub fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        BemuProfile::new(self.profile).report(total, self.cpu_elapsed)
    }
}
