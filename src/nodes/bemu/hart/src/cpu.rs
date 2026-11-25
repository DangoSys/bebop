use super::call_profile::{CallProfile, Context as CallContext};
use super::process::{write_guest, Process};
use super::HartState;
use bebop_bemu_profile::{BemuProfile, BemuProfileReport};
use bebop_elf::{analyze_elf, load_elf, LoadInfo};
use generic::riscv::{
    mmu::Mmu,
    registers::{Pace, Registers, Step},
};
use generic::{
    configs,
    ddr::{DRAM_BASE, DTB_ADDRESS, INITRD_ADDRESS},
    guest::GuestAccess,
    hlink::{HLink, Port},
    rocc::Rocc,
};
use snafu::{FromString, OptionExt, ResultExt, Whatever};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// A hart's RV64 CPU: it runs the program and hands each custom-3 instruction
/// to what the hart connects to its RoCC port.
pub struct Cpu {
    registers: Registers,
    mmu: Mmu,
    hart: Arc<HartState>,
    process: Option<Process>,
    firmware_io: Option<Process>,
    loaded_elf: Option<LoadInfo>,
    image_physical: (u64, u64),
    hart_id: usize,
    disasm: Option<BufWriter<File>>,
    /// Hart dataflow storage commands (operations 20..23), with --itrace.
    dataflow: Option<BufWriter<File>>,
    profile: bool,
    instruction_profile: BTreeMap<u64, (u64, u64)>,
    instruction_profile_path: PathBuf,
    call_profile: CallProfile,
    call_log: Option<File>,
    profile_active_cycles: u64,
    profile_instructions: u64,
    cpu_elapsed: Duration,
    elapsed_cycles: u64,
    timing: Box<dyn generic::timing::CpuTiming>,
    instruction_cycles: u64,
    next_timer_tick: u64,
    parallel: bool,
    throttled: bool,
    exit_code: Option<i32>,
    waiting: bool,
    service_address: Option<u64>,
}

impl Cpu {
    pub fn hart_id(&self) -> usize {
        self.registers.id as usize
    }
    pub fn new(
        device: &HLink,
        params: &generic::configs::CoreParams,
        options: &generic::options::ModelOptions,
        dma: generic::timing::DmaLayout,
        timing: Box<dyn generic::timing::CpuTiming>,
    ) -> Result<Self, Whatever> {
        let log_dir = &options.log_dir;
        let disasm = options.disasm;
        let profile = options.profile;
        let endpoint = configs::Endpoint::Hart(params.hart_id);
        configs::configure(endpoint);
        std::fs::create_dir_all(log_dir).whatever_context("create hart log directory")?;
        let hart = HartState::new(device, params, dma);
        let hart_id = params.hart_id;
        let registers = Registers::new(hart_id as u64, DRAM_BASE);
        let mmu = Mmu::default();
        hart.link.platform.lock().expect("BEMU platform poisoned").uart_log(
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
        let dataflow = if options.trace.itrace {
            Some(BufWriter::new(
                File::create(log_dir.join("dataflow.ndjson")).whatever_context("create dataflow log")?,
            ))
        } else {
            None
        };
        Ok(Self {
            registers,
            mmu,
            hart,
            process: Some(Process::new()),
            firmware_io: None,
            loaded_elf: None,
            image_physical: (DRAM_BASE, 0),
            hart_id,
            disasm,
            dataflow,
            profile,
            instruction_profile: BTreeMap::new(),
            instruction_profile_path: log_dir.join("cpu-instructions.csv"),
            call_profile: CallProfile::default(),
            call_log: None,
            profile_active_cycles: 0,
            profile_instructions: 0,
            cpu_elapsed: Duration::ZERO,
            elapsed_cycles: 0,
            timing,
            instruction_cycles: 1,
            next_timer_tick: generic::clint::TICK_CYCLES,
            parallel: device.capacity > 1,
            throttled: false,
            exit_code: None,
            waiting: false,
            service_address: None,
        })
    }

    pub fn load_elf(&mut self, elf: &Path) -> Result<(), Whatever> {
        configs::configure(configs::Endpoint::Hart(self.hart_id));
        let path = elf.to_str().whatever_context("invalid ELF path")?;
        self.process
            .as_mut()
            .expect("process must exist before loading firmware")
            .program_name = path.to_owned();
        let mut platform = self.hart.link.platform.lock().expect("BEMU platform poisoned");
        platform.capture_output(bebop_elf::OutputCapture::from_elf(elf).map_err(Whatever::without_source)?);
        let analysis = analyze_elf(path, DRAM_BASE).map_err(Whatever::without_source)?;
        if analysis.os_abi == bebop_elf::OsAbi::GnuUser {
            let bytes = crate::process::align_up(analysis.image_end - DRAM_BASE, crate::process::PAGE_SIZE);
            let address = self
                .hart
                .pages
                .lock()
                .expect("DDR page pool poisoned")
                .allocate(bytes)
                .map_err(Whatever::without_source)?;
            let offset = (address - DRAM_BASE) as usize;
            self.image_physical = (address, bytes);
            let memory = self.hart.link.memory.as_ref();
            memory.fill(address, bytes as usize, 0).expect("ELF image range");
            self.loaded_elf = Some(load_elf(path, memory, DRAM_BASE, offset).map_err(Whatever::without_source)?);
        } else {
            self.loaded_elf =
                Some(load_elf(path, self.hart.link.memory.as_ref(), DRAM_BASE, 0).map_err(Whatever::without_source)?);
        }
        self.process
            .as_mut()
            .expect("ELF must be loaded before firmware initialization")
            .syscall
            .working_dir = elf.parent().expect("ELF parent directory").to_path_buf();
        drop(platform);
        if self.profile {
            let scopes = bebop_elf::read_cpu_profile_scopes(elf).map_err(Whatever::without_source)?;
            self.install_call_profiles(&scopes)?;
        }
        Ok(())
    }

    pub fn install_call_profiles(&mut self, scopes: &[bebop_elf::CpuProfileScope]) -> Result<(), Whatever> {
        self.call_profile = CallProfile::new(scopes);
        self.profile_active_cycles = 0;
        self.profile_instructions = 0;
        self.call_log = if scopes.is_empty() {
            None
        } else {
            Some(
                File::create(self.instruction_profile_path.with_file_name("cpu-calls.jsonl"))
                    .whatever_context("create CPU call profile")?,
            )
        };
        Ok(())
    }

    /// The hart converts between half precision and the wider formats in its FPU.
    pub fn enable_half_precision(&mut self) {
        self.registers.half_precision = true;
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
        configs::configure(configs::Endpoint::Hart(self.hart_id));
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load ELF before initializing the registers")?;
        let _platform = self.hart.link.platform.lock().expect("BEMU platform poisoned");
        let pages = Arc::clone(&self.hart.pages);
        self.process
            .as_mut()
            .expect("process mode is unavailable")
            .initialize(
                &mut self.registers,
                self.hart.link.memory.as_ref(),
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
            write_guest(self.hart.link.memory.as_ref(), DTB_ADDRESS, &bytes).map_err(Whatever::without_source)?;
        }
        if let Some(path) = initrd {
            let image = std::fs::read(path).whatever_context("read initramfs")?;
            write_guest(self.hart.link.memory.as_ref(), INITRD_ADDRESS, &image).map_err(Whatever::without_source)?;
        }
        self.init_firmware_hart(load.entry, if dtb.is_some() { DTB_ADDRESS } else { 0 });
        Ok(())
    }

    pub fn init_firmware_hart(&mut self, entry: u64, dtb: u64) {
        self.process = None;
        self.registers.privilege = generic::riscv::Privilege::Machine;
        self.registers.pc = entry;
        self.registers.set_register(10, self.registers.id);
        self.registers.set_register(11, dtb);
    }

    /// A device or thread management instruction. None is a service payload stalled on a page fault.
    fn control(&mut self, operation: u32, a: u64, b: u64) -> Result<Option<u64>, Whatever> {
        if (20..=23).contains(&operation) {
            let value = self
                .hart
                .link
                .thread_memory
                .get()
                .expect("Hart has no dataflow service")
                .control(operation, a, b);
            if let Some(log) = &mut self.dataflow {
                writeln!(
                    log,
                    r#"{{"type":"dataflow","cycle":{},"operation":{},"field":{},"value":{},"result":{}}}"#,
                    self.elapsed_cycles, operation, a, b, value
                )
                .whatever_context("write dataflow log")?;
                // Worker harts never exit; keep the log complete.
                log.flush().whatever_context("flush dataflow log")?;
            }
            return Ok(Some(value));
        }
        if operation == 2 {
            let thread = (a >> 32) as usize;
            let task = self.hart.link.tasks.contexts[thread].control.lock().unwrap().descriptor[0];
            if let Some(memory) = self.hart.link.thread_memory.get() {
                memory.start(thread, task);
            }
        }
        let before = self.elapsed_cycles;
        let value = if operation == 19 {
            assert!(a <= 1 && b > 0 && b <= 65536, "Invalid service payload");
            let address = self.service_address.expect("Service payload has no address");
            let to_device = a == 1;
            let translation = self.registers.translation_context();
            let satp = translation.satp.read();
            let mut guest = GuestAccess::new(
                Port {
                    cache: None,
                    devices: &self.hart.link.platform,
                    memory: &self.hart.link.memory,
                    peripheral: false,
                },
                &self.mmu,
                translation,
                &self.hart.link,
                0,
            );
            let access = if to_device {
                generic::riscv::Access::Load
            } else {
                generic::riscv::Access::Store
            };
            // The payload is a DMA of the hart: a page fault stalls the instruction and raises
            // interrupt 16, and the instruction runs again once its owner resolved the fault.
            if let Err(trap) = guest.check_range(address, b as usize, access) {
                if !matches!(trap.cause, 13 | 15) || self.process.is_some() {
                    return Err(Whatever::without_source(format!("Service payload fault: {trap:?}")));
                }
                self.registers.csrs.dma_fault = Some(generic::riscv::csr::DmaFault {
                    address: trap.value,
                    store: trap.cause == 15,
                    satp,
                });
                return Ok(None);
            }
            self.service_address = None;
            let mut bytes = vec![0; b as usize];
            if to_device {
                guest.read_buffer(address, &mut bytes);
            }
            let phases = self
                .hart
                .service
                .as_ref()
                .expect("Hart has no device service")
                .exchange(self.hart.link.hart_id, to_device, &mut bytes);
            if !to_device {
                guest.write_buffer(address, &bytes);
            }
            // ReadTransport reissues single-beat reads for each unaligned row.
            // The first transaction is already included in transfer_cycles.
            let unaligned = if to_device && address & 15 != 0 {
                (b.div_ceil(16) - 1) * 48
            } else {
                0
            };
            self.elapsed_cycles += if to_device && address & 15 == 0 {
                guest.stage_cycles(self.elapsed_cycles, &phases)
            } else if to_device {
                // Unaligned ReadTransport reissues single-beat reads. Its conservative
                // timing is retained until that separate state path is qualified.
                phases.stream * 4 + unaligned + guest.transfer_cycles(self.elapsed_cycles, b.div_ceil(16), false)
            } else {
                phases.prepare
                    + phases.stream
                    + phases.retire
                    + guest.transfer_cycles(self.elapsed_cycles, b.div_ceil(16), true)
            };
            0
        } else if (13..=18).contains(&operation) {
            let a = if operation == 18 {
                assert!(
                    self.service_address.replace(a).is_none(),
                    "Service payload already staged"
                );
                0
            } else {
                a
            };
            let (value, cycles) = self.hart.service.as_ref().expect("Hart has no device service").request(
                self.hart.link.hart_id,
                operation,
                a,
                b,
            );
            self.elapsed_cycles += cycles;
            value
        } else {
            self.hart.link.tasks.control(
                operation,
                a,
                b,
                &self.registers,
                &self.hart.link.clint,
                &mut self.elapsed_cycles,
            )
        };
        if self.registers.csrs.inhibit & 1 == 0 {
            self.registers.csrs.cycle = self.registers.csrs.cycle.wrapping_add(self.elapsed_cycles - before);
        }
        self.hart.link.clint.advance_to(self.hart_id, self.elapsed_cycles);
        Ok(Some(value))
    }

    /// The state the hart shares with what is connected to it.
    pub fn state(&self) -> &Arc<HartState> {
        &self.hart
    }

    pub fn step(&mut self, count: u64, rocc: &mut dyn Rocc) -> Result<(), Whatever> {
        configs::configure(configs::Endpoint::Hart(self.hart_id));
        self.throttled = false;
        // Bound temporal skew without synchronizing host threads per instruction.
        let deadline = if self.parallel {
            self.hart.link.clint.published_cycles() + generic::clint::CPU_SKEW
        } else {
            u64::MAX
        };
        // The accelerator's event times and its barrier change only at a custom instruction.
        let mut accelerator = rocc.cycles();
        let mut barrier = rocc.barrier();
        let mut remaining = count;
        while remaining != 0 {
            let code = self.hart.exit_code.load(std::sync::atomic::Ordering::Acquire);
            if code != i64::MIN {
                self.hart.link.clint.halt();
                self.exit_code = Some(code as i32);
            }
            if self.exit_code.is_some() {
                break;
            }
            if !self.waiting && self.elapsed_cycles >= deadline {
                self.throttled = true;
                break;
            }
            let next_cycle = if self.waiting {
                self.elapsed_cycles.max(self.hart.link.clint.cycles())
            } else {
                self.elapsed_cycles + self.instruction_cycles
            };
            let cycles = next_cycle - self.elapsed_cycles;
            self.elapsed_cycles = next_cycle;
            // Publish at timer boundaries and scheduling events, not at every instruction.
            if self.elapsed_cycles >= self.next_timer_tick {
                let tick = generic::clint::TICK_CYCLES;
                self.next_timer_tick = (self.elapsed_cycles / tick + 1) * tick;
                self.hart.link.clint.advance_to(self.hart_id, self.elapsed_cycles);
            }
            let clock = std::cell::Cell::new(self.elapsed_cycles);
            let inputs = crate::input::Inputs {
                hart: self.registers.id as usize,
                clint: &self.hart.link.clint,
                clock: &clock,
                memory_ready: accelerator.1,
            };
            let pc = self.registers.pc;
            let call_context = (self.profile && self.call_profile.interesting(pc)).then(|| CallContext {
                pc,
                ra: self.registers.register(1),
                arguments: std::array::from_fn(|i| self.registers.register(10 + i)),
                instructions: self.profile_instructions,
                cycle: self.elapsed_cycles,
            });
            let privilege = self.registers.privilege;
            let counting_cycles = self.registers.csrs.inhibit & 1 == 0;
            let started = self.profile.then(Instant::now);
            // The registers run on by themselves to the next event, the next timer boundary or the skew bound;
            // one instruction at a time where each is observed, or the accelerator holds the hart at a barrier.
            let limit = if self.profile || self.disasm.is_some() || barrier {
                1
            } else {
                remaining
            };
            let mut pace = Pace {
                clock: &clock,
                instruction_cycles: self.instruction_cycles,
                horizon: deadline.min(self.next_timer_tick),
                limit,
            };
            let mut event = self.registers.run(
                &mut Port {
                    cache: Some(&self.hart.cache),
                    devices: &self.hart.link.platform,
                    memory: &self.hart.link.memory,
                    peripheral: false,
                },
                &self.mmu,
                &inputs,
                &mut *self.timing,
                cycles,
                &mut pace,
            );
            remaining -= limit - pace.limit;
            self.elapsed_cycles = clock.get();
            self.instruction_cycles = pace.instruction_cycles;
            if let Some(started) = started {
                self.cpu_elapsed += started.elapsed();
            }
            // The run times what it retires; a custom instruction retires here.
            let custom = matches!(event, Step::Custom(_));
            if let Step::Custom(request) = event {
                self.hart.link.clint.advance_to(self.hart_id, self.elapsed_cycles);
                let value = match request.instruction & 0x7f {
                    // An accelerator stalled on a DMA page fault takes no
                    // instruction until the registers resolves the fault.
                    0x7b if self.registers.csrs.dma_fault.is_some() => None,
                    0x7b => {
                        let translation = self.registers.translation_context();
                        let satp = translation.satp.read();
                        let guest = GuestAccess::new(
                            Port {
                                cache: None,
                                devices: &self.hart.link.platform,
                                memory: &self.hart.link.memory,
                                peripheral: false,
                            },
                            &self.mmu,
                            translation,
                            &self.hart.link,
                            0,
                        );
                        // Every page an instruction's DMA touches is translated
                        // before it has any effect: a page fault stalls the
                        // instruction and raises interrupt 16.
                        match rocc.custom(guest, request, self.elapsed_cycles) {
                            Err(trap) if matches!(trap.cause, 13 | 15) => {
                                if self.process.is_some() {
                                    return Err(Whatever::without_source(format!(
                                        "accelerator DMA page fault at 0x{:x} without an OS to resolve it",
                                        trap.value
                                    )));
                                }
                                self.registers.csrs.dma_fault = Some(generic::riscv::csr::DmaFault {
                                    address: trap.value,
                                    store: trap.cause == 15,
                                    satp,
                                });
                                None
                            }
                            result => Some(result),
                        }
                    }
                    0x0b => {
                        let guest = GuestAccess::new(
                            Port {
                                cache: None,
                                devices: &self.hart.link.platform,
                                memory: &self.hart.link.memory,
                                peripheral: false,
                            },
                            &self.mmu,
                            self.registers.translation_context(),
                            &self.hart.link,
                            0,
                        );
                        Some(rocc.custom0(guest, request, self.elapsed_cycles))
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
                        self.control(operation, request.rs1, request.rs2)?.map(Ok)
                    }
                    _ => Some(Err(generic::riscv::Trap::illegal(request.instruction))),
                };
                event = match value {
                    Some(value) => self
                        .registers
                        .complete_custom(value.map(|value| (request.instruction & (1 << 14) != 0).then_some(value))),
                    None => self.registers.stall_custom(),
                };
                accelerator = rocc.cycles();
                barrier = rocc.barrier();
            }
            let waiting = event == Step::Waiting;
            if waiting != self.waiting {
                self.hart.link.clint.advance_to(self.hart_id, self.elapsed_cycles);
                self.hart
                    .link
                    .clint
                    .set_waiting(self.hart_id, waiting, self.registers.csrs.ie);
                self.waiting = waiting;
            }

            match event {
                Step::Retired {
                    pc,
                    instruction,
                    decoded,
                    operand,
                } => {
                    let resume = match instruction & 0x7f {
                        0x0b | 0x7b => accelerator.0,
                        0x0f => accelerator.1,
                        _ => self.elapsed_cycles,
                    };
                    let stalled = resume.saturating_sub(self.elapsed_cycles);
                    self.elapsed_cycles += stalled;
                    if counting_cycles {
                        self.registers.csrs.cycle = self.registers.csrs.cycle.wrapping_add(stalled);
                    }
                    if stalled != 0 {
                        self.hart.link.clint.advance_to(self.hart_id, self.elapsed_cycles);
                    }
                    if custom {
                        self.instruction_cycles = self.timing.instruction(pc, instruction, decoded, operand);
                    }
                    if self.profile {
                        if let Some(context) = call_context {
                            let samples = self.call_profile.observe(context, self.profile_active_cycles);
                            if let Some(log) = &mut self.call_log {
                                for sample in samples {
                                    let mut line =
                                        serde_json::to_vec(&sample).whatever_context("serialize CPU call profile")?;
                                    line.push(b'\n');
                                    log.write_all(&line).whatever_context("write CPU call profile")?;
                                }
                            }
                        }
                        self.profile_active_cycles += self.instruction_cycles;
                        self.profile_instructions += 1;
                        let entry = self.instruction_profile.entry(pc).or_default();
                        entry.0 += 1;
                        entry.1 += self.instruction_cycles;
                    }
                    if let Some(log) = &mut self.disasm {
                        writeln!(
                            log,
                            "registers={} privilege={privilege:?} pc=0x{pc:016x} inst=0x{instruction:08x} {}",
                            self.registers.id,
                            bebop_dasm::disassemble(decoded)
                        )
                        .whatever_context("write instruction log")?;
                    }
                }
                Step::Trap(trap) => {
                    if let Some(log) = &mut self.disasm {
                        writeln!(
                            log,
                            "registers={} pc=0x{pc:016x} trap={} value=0x{:x}",
                            self.registers.id, trap.cause, trap.value
                        )
                        .whatever_context("write trap log")?;
                    }
                    if let Some(process) = &mut self.process {
                        if trap.cause == if process.user_mode { 8 } else { 11 } {
                            process
                                .syscall(&mut self.registers, &self.hart.link.platform)
                                .map_err(Whatever::without_source)?;
                            self.hart.link.memory.clear_reservations();
                            if self.registers.register(17) == 124 {
                                std::thread::yield_now();
                            }
                            if matches!(self.registers.register(17), 214 | 215 | 222) {
                                self.mmu.fence();
                            }
                            self.exit_code = process.syscall.exit_code;
                        } else if process.user_mode {
                            return Err(Whatever::without_source(format!(
                                "user trap at pc=0x{:x}: cause={} value=0x{:x}",
                                self.registers.csrs.epc[1], trap.cause, trap.value
                            )));
                        }
                    } else if trap.cause == 11 && self.registers.register(16) == 0x4245_4d55_494f_0001 {
                        // Explicit BEMU preparation programs can use host I/O
                        // without changing ordinary firmware trap handling.
                        let io = self.firmware_io.get_or_insert_with(Process::new);
                        io.syscall.working_dir = self.instruction_profile_path.parent().unwrap().to_path_buf();
                        io.syscall(&mut self.registers, &self.hart.link.platform)
                            .map_err(Whatever::without_source)?;
                        self.hart.link.memory.clear_reservations();
                    }
                }
                Step::Waiting => break,
                Step::Stalled => {}
                Step::Custom(_) => unreachable!(),
            }
            if barrier {
                break;
            }
        }
        self.hart.link.clint.advance_to(self.hart_id, self.elapsed_cycles);
        if self.exit_code.is_some() {
            self.hart.link.clint.finish(self.hart_id);
            self.hart
                .link
                .platform
                .lock()
                .expect("BEMU platform poisoned")
                .flush_uart(self.registers.id as usize)
                .whatever_context("flush UART log")?;
            if let Some(log) = &mut self.disasm {
                log.flush().whatever_context("flush instruction log")?;
            }
        }
        Ok(())
    }

    pub fn wait_for_event(&mut self) {
        if self.throttled {
            self.hart
                .link
                .clint
                .wait_for_progress(self.hart_id, self.elapsed_cycles - generic::clint::CPU_SKEW);
        }
        if !self.waiting {
            return;
        }
        self.hart
            .link
            .clint
            .advance_to(self.hart_id, self.hart.link.clint.cycles());
        self.hart.link.clint.wait(self.hart_id, self.registers.csrs.ie);
        let now = self.elapsed_cycles.max(self.hart.link.clint.cycles());
        if self.registers.csrs.inhibit & 1 == 0 {
            self.registers.csrs.cycle = self.registers.csrs.cycle.wrapping_add(now - self.elapsed_cycles);
        }
        self.elapsed_cycles = now;
        self.hart.link.clint.advance_to(self.hart_id, now);
        self.waiting = false;
        self.instruction_cycles = 1;
        self.hart
            .link
            .clint
            .set_waiting(self.hart_id, false, self.registers.csrs.ie);
    }

    pub fn stop(&mut self, code: i32) {
        self.exit_code = Some(code);
        self.hart.link.clint.finish(self.hart_id);
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
        (self.registers.csrs.instret, self.elapsed_cycles)
    }
    pub fn write_instruction_profile(&self) {
        std::fs::write(
            self.instruction_profile_path.with_file_name("cpu-state.log"),
            format!("pc=0x{:x} elapsed_cycles={} mcycle={} waiting={} throttled={} mie=0x{:x} pending=0x{:x} coordinated_cycles={}\n",
                self.registers.pc, self.elapsed_cycles, self.registers.csrs.cycle,
                self.waiting, self.throttled, self.registers.csrs.ie,
                self.hart.link.clint.sample(self.hart_id).1, self.hart.link.clint.published_cycles()),
        ).unwrap();
        if self.profile {
            let mut file = BufWriter::new(File::create(&self.instruction_profile_path).unwrap());
            writeln!(file, "pc,instructions,modeled_active_cycles").unwrap();
            for (pc, (instructions, cycles)) in &self.instruction_profile {
                writeln!(file, "{pc:#x},{instructions},{cycles}").unwrap();
            }
        }
    }
    pub fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        BemuProfile::new(self.profile).report(total, self.cpu_elapsed)
    }
}
