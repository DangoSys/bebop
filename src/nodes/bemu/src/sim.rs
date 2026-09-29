use crate::{
    accel,
    config,
    process::{write_guest, Process},
    root::{
        chip::Chip,
        mmu::GuestAccess,
        platform::{Port, DRAM_BASE, DTB_ADDRESS, INITRD_ADDRESS},
        tile::{tasks::Task, Tile},
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
    process: Option<Process>,
    loaded_elf: Option<LoadInfo>,
    image_physical: (u64, u64),
    core_index: usize,
    disasm: Option<BufWriter<File>>,
    profile: bool,
    cpu_elapsed: Duration,
    exit_code: Option<i32>,
    waiting: bool,
    task_mode: bool,
    task_done: bool,
    task_completion: u64,
}

impl Core {
    pub fn chip(&self) -> Chip {
        Chip {
            platform: Arc::clone(&self.tile.platform),
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
        Self::new_with_core_hart(log_dir, trace, disasm, profile, core_index, 0, None)
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
        let accel =
            accel::State::new(log_dir, trace, profile, hart_id, shared.clone()).map_err(Whatever::without_source)?;
        let tile = match shared {
            Some(tile) => tile,
            None => Tile::new(
                &Chip::new(3 * (1 << 30), hart_id + 1),
                0,
                hart_id + 1,
                Vec::new(),
                0,
                0,
                config::virtual_bank_num(),
            ),
        };
        let hart_id = tile.first_hart + hart_id;
        tile.platform.lock().expect("BEMU platform poisoned").uart_log(
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
        let mut hart = Hart::new(hart_id as u64, DRAM_BASE);
        if config::vector_len() != 0 {
            hart.csrs.vector = Some(rvsim::vector::VectorConfig::new(config::vector_len()));
        }
        Ok(Self {
            hart,
            mmu: Mmu::default(),
            tile,
            accel,
            process: Some(Process::new()),
            loaded_elf: None,
            image_physical: (DRAM_BASE, 0),
            core_index,
            disasm,
            profile,
            cpu_elapsed: Duration::ZERO,
            exit_code: None,
            waiting: false,
            task_mode: false,
            task_done: false,
            task_completion: 0,
        })
    }

    pub fn load_elf(&mut self, elf: &Path, pk: bool) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        let path = elf.to_str().whatever_context("invalid ELF path")?;
        self.process
            .as_mut()
            .expect("process must exist before loading firmware")
            .program_name = path.to_owned();
        let mut platform = self.tile.platform.lock().expect("BEMU platform poisoned");
        if pk {
            let analysis = analyze_elf(path, DRAM_BASE).map_err(Whatever::without_source)?;
            let bytes = crate::process::align_up(analysis.image_end - DRAM_BASE, crate::process::PAGE_SIZE);
            let address = platform.pages.lock().expect("DDR page pool poisoned")
                .allocate(bytes).map_err(Whatever::without_source)?;
            let offset = (address - DRAM_BASE) as usize;
            self.image_physical = (address, bytes);
            platform.memory[offset..offset + bytes as usize].fill(0);
            self.loaded_elf = Some(load_elf(path, &mut platform.memory[offset..offset + bytes as usize], DRAM_BASE)
                .map_err(Whatever::without_source)?);
        } else {
            self.loaded_elf = Some(load_elf(path, &mut platform.memory, DRAM_BASE).map_err(Whatever::without_source)?);
        }
        self.process
            .as_mut()
            .expect("ELF must be loaded before system initialization")
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

    pub(crate) fn set_stdio(&mut self, stream: std::os::unix::net::UnixStream) -> Result<(), String> {
        self.process.as_mut().expect("stdio requires process mode").streams.connect(stream, self.hart.id as usize).map_err(|e| e.to_string())
    }

    pub fn init_hart(&mut self, pk: bool) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load ELF before initializing the hart")?;
        let mut platform = self.tile.platform.lock().expect("BEMU platform poisoned");
        let pages = Arc::clone(&platform.pages);
        self.process
            .as_mut()
            .expect("process mode is unavailable")
            .initialize(&mut self.hart, &mut platform.memory, load, pk, pages, self.image_physical)
            .map_err(Whatever::without_source)
    }

    pub fn init_system(&mut self, dtb: &Path, initrd: Option<&Path>) -> Result<(), Whatever> {
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load firmware ELF before initializing the system")?;
        let mut platform = self.tile.platform.lock().expect("BEMU platform poisoned");
        let dtb = std::fs::read(dtb).whatever_context("read system DTB")?;
        write_guest(&mut platform.memory, DTB_ADDRESS, &dtb).map_err(Whatever::without_source)?;
        if let Some(path) = initrd {
            let image = std::fs::read(path).whatever_context("read initramfs")?;
            write_guest(&mut platform.memory, INITRD_ADDRESS, &image).map_err(Whatever::without_source)?;
        }
        self.process = None;
        self.hart.pc = load.entry;
        self.hart.set_register(10, self.hart.id);
        self.hart.set_register(11, DTB_ADDRESS);
        Ok(())
    }

    pub(crate) fn start_task(&mut self, task: Task) {
        config::configure_core(self.core_index);
        let id = self.hart.id;
        let cycle = self.hart.csrs.read(0xb00, rvsim::Privilege::Machine, id, 0, 0).unwrap();
        let retired = self.hart.csrs.read(0xb02, rvsim::Privilege::Machine, id, 0, 0).unwrap();
        self.task_completion = task.completion;
        self.hart = Hart::new(id, task.entry);
        self.hart.csrs = task.csrs;
        self.hart.csrs.write(0xb00, cycle, rvsim::Privilege::Machine).unwrap();
        self.hart.csrs.write(0xb02, retired, rvsim::Privilege::Machine).unwrap();
        self.hart.csrs.vector =
            (config::vector_len() != 0).then(|| rvsim::vector::VectorConfig::new(config::vector_len()));
        self.hart.privilege = task.privilege;
        self.hart.set_register(2, task.stack);
        self.hart.set_register(3, task.gp);
        self.hart.set_register(4, task.tls);
        self.hart.set_register(10, task.argument);
        self.process = None;
        self.waiting = false;
        self.task_mode = true;
        self.task_done = false;
        self.mmu.fence();
    }

    pub(crate) fn task_done(&self) -> bool {
        self.task_done
    }

    pub(crate) fn complete_task(&mut self, status: u64) {
        let translation = rvsim::mmu::TranslationContext {
            privilege: rvsim::Privilege::User,
            satp: self.hart.csrs.satp,
            mstatus: 0,
            pmp: &self.hart.csrs.pmp,
        };
        let mut memory = GuestAccess {
            platform: Port(&self.tile.platform),
            mmu: &self.mmu,
            translation,
        };
        self.tile.tasks.complete(self.accel.hart_id - 1, status);
        memory.write_buffer(
            self.task_completion,
            &(if status == 0 { 1_u64 } else { 2_u64 }).to_le_bytes(),
        );
        self.task_done = true;
    }

    fn control(&mut self, operation: u32, a: u64, b: u64) -> Result<u64, Whatever> {
        let local_hart = self.accel.hart_id;
        match operation {
            0 if local_hart == 0 && self.hart.privilege == rvsim::Privilege::User => {
                let mut words = [0_u64; 7];
                let mut memory = GuestAccess {
                    platform: Port(&self.tile.platform),
                    mmu: &self.mmu,
                    translation: self.hart.translation_context(),
                };
                for (index, word) in words.iter_mut().enumerate() {
                    let mut bytes = [0; 8];
                    memory.read_buffer(b + index as u64 * 8, &mut bytes);
                    *word = u64::from_le_bytes(bytes);
                }
                let task = Task {
                    entry: words[0],
                    argument: words[1],
                    stack: words[2],
                    tls: words[3],
                    gp: self.hart.register(3),
                    workspace: words[4],
                    completion: words[5],
                    csrs: self.hart.csrs.clone(),
                    privilege: self.hart.privilege,
                };
                self.tile
                    .tasks
                    .submit(a as usize, words[6], task)
                    .map_err(Whatever::without_source)?;
                Ok(0)
            }
            1 if local_hart == 0 => Ok(self.tile.tasks.wait(a as usize)),
            2 if self.task_mode => {
                self.complete_task(a);
                Ok(0)
            }
            3 => Ok(self.tile.tasks.signatures.len() as u64),
            4 => Ok(self.tile.tasks.signatures[a as usize]),
            5 => Ok(self.tile.tasks.workspace(local_hart)),
            6 if local_hart == 0 => {
                self.tile.tasks.set_workspace(a);
                Ok(0)
            }
            _ => Err(Whatever::without_source(format!(
                "invalid tile control operation {operation} on hart {}",
                self.hart.id
            ))),
        }
    }

    pub fn step(&mut self, count: u64) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        for _ in 0..count {
            if self.exit_code.is_some() {
                break;
            }
            let cycles = if self.waiting { 10_000 } else { 1 };
            let inputs = self
                .tile
                .platform
                .lock()
                .expect("BEMU platform poisoned")
                .inputs(self.hart.id as usize, cycles);
            let pc = self.hart.pc;
            let started = self.profile.then(Instant::now);
            let mut event = self.hart.step(&mut Port(&self.tile.platform), &self.mmu, inputs);
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
                        let guest = GuestAccess {
                            platform: Port(&self.tile.platform),
                            mmu: &self.mmu,
                            translation,
                        };
                        accel::execute(
                            &mut self.accel,
                            guest,
                            (request.instruction >> 25) as u8,
                            request.rs1,
                            request.rs2,
                            request.pc,
                        )
                    }
                    0x2b => self.control(request.instruction >> 25, request.rs1, request.rs2)?,
                    _ => {
                        return Err(Whatever::without_source(format!(
                            "unsupported custom opcode: 0x{:08x}",
                            request.instruction
                        )))
                    }
                };
                event = self
                    .hart
                    .complete_custom(Ok((request.instruction & (1 << 14) != 0).then_some(value)));
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
                            "hart={} pc=0x{pc:016x} inst=0x{instruction:08x} {}",
                            self.hart.id,
                            bebop_dasm::disassemble(decoded)
                        )
                        .whatever_context("write instruction log")?;
                    }
                }
                Step::Trap(trap) => {
                    if self.task_mode {
                        return Err(Whatever::without_source(format!(
                            "task trap at pc=0x{pc:x}: cause={} value=0x{:x}",
                            trap.cause, trap.value
                        )));
                    }
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
                            let mut platform = self.tile.platform.lock().expect("BEMU platform poisoned");
                            platform.reservations.clear();
                            drop(platform);
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
                Step::Waiting => (),
                Step::Custom(_) => unreachable!(),
            }
            if let Some(code) =
                self.tile.platform.lock().expect("BEMU platform poisoned").exit_codes[self.hart.id as usize]
            {
                self.exit_code = Some(code);
            }
            if self.accel.barrier_hit || self.task_done {
                break;
            }
        }
        if self.exit_code.is_some() {
            self.tile
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
    pub fn total_latency(&self) -> u64 {
        self.accel.total_lat
    }
    pub fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        self.accel.profile.report(total, self.cpu_elapsed)
    }
}
