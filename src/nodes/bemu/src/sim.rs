use crate::{
    accel, config,
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
    worker_index: Option<usize>,
    is_controller: bool,
    disasm: Option<BufWriter<File>>,
    profile: bool,
    cpu_elapsed: Duration,
    elapsed_cycles: u64,
    exit_code: Option<i32>,
    waiting: bool,
    task_mode: bool,
    task_done: bool,
    firmware_task: Option<Task>,
    pending_control: Option<rvsim::hart::CustomInstruction>,
}

impl Core {
    pub fn hart_id(&self) -> usize { self.hart.id as usize }
    pub fn chip(&self) -> Chip {
        Chip {
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
        let tile = shared.unwrap_or_else(|| Tile::new(
            &Chip::new(3 * (1 << 30), config::hart_capacity()),
            &config::tile_for_core(core_index), Vec::new()));
        assert!(tile.harts.contains(&hart_id), "core does not belong to Tile");
        let worker_index = tile.worker_harts.iter().position(|&id| id == hart_id);
        let endpoint_index = tile.endpoint_harts.iter().position(|&id| id == hart_id);
        let is_controller = tile.controller_hart == Some(hart_id);
        let accel = accel::State::new(log_dir, trace, profile, hart_id,
            endpoint_index, Some(Arc::clone(&tile))).map_err(Whatever::without_source)?;
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
            process: Some(Process::new()),
            loaded_elf: None,
            image_physical: (DRAM_BASE, 0),
            core_index,
            worker_index,
            is_controller,
            disasm,
            profile,
            cpu_elapsed: Duration::ZERO,
            elapsed_cycles: 0,
            exit_code: None,
            waiting: false,
            task_mode: false,
            task_done: false,
            firmware_task: None,
            pending_control: None,
        })
    }

    pub fn load_elf(&mut self, elf: &Path, pk: bool) -> Result<(), Whatever> {
        config::configure_core(self.core_index);
        let path = elf.to_str().whatever_context("invalid ELF path")?;
        self.process
            .as_mut()
            .expect("process must exist before loading firmware")
            .program_name = path.to_owned();
        let platform = self.tile.platform.lock().expect("BEMU platform poisoned");
        if pk {
            let analysis = analyze_elf(path, DRAM_BASE).map_err(Whatever::without_source)?;
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
        self.process
            .as_mut()
            .expect("stdio requires process mode")
            .streams
            .connect(stream, self.hart.id as usize)
            .map_err(|e| e.to_string())
    }

    pub fn init_hart(&mut self, pk: bool) -> Result<(), Whatever> {
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
                pk,
                pages,
                self.image_physical,
            )
            .map_err(Whatever::without_source)
    }

    pub fn init_system(&mut self, dtb: &Path, initrd: Option<&Path>) -> Result<(), Whatever> {
        let load = self
            .loaded_elf
            .take()
            .whatever_context("load firmware ELF before initializing the system")?;
        let dtb = std::fs::read(dtb).whatever_context("read system DTB")?;
        write_guest(self.tile.memory.as_ref(), DTB_ADDRESS, &dtb).map_err(Whatever::without_source)?;
        if let Some(path) = initrd {
            let image = std::fs::read(path).whatever_context("read initramfs")?;
            write_guest(self.tile.memory.as_ref(), INITRD_ADDRESS, &image).map_err(Whatever::without_source)?;
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

        self.hart = Hart::new(id, task.entry);
        self.hart.csrs = task.csrs;
        self.hart.csrs.write(0xb00, cycle, rvsim::Privilege::Machine).unwrap();
        self.hart.csrs.write(0xb02, retired, rvsim::Privilege::Machine).unwrap();

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
        self.tile.tasks.complete(self.worker_index.expect("task completion requires task worker"), status);
        self.task_done = true;
    }

    fn control(&mut self, operation: u32, a: u64, b: u64) -> Result<u64, Whatever> {
        let worker = self.worker_index;
        match operation {
            0 if self.is_controller => {
                let words = self.tile.tasks.descriptor().map_err(Whatever::without_source)?;
                if words[6] != b { return Err(Whatever::without_source("task signature differs from descriptor".into())); }
                let task = Task {
                    entry: words[0],
                    argument: words[1],
                    stack: words[2],
                    tls: words[3],
                    gp: words[5],
                    workspace: words[4],
                    csrs: self.hart.csrs.clone(),
                    privilege: self.hart.privilege,
                };
                self.tile
                    .tasks
                    .submit(a as usize, words[6], task)
                    .map_err(Whatever::without_source)?;
                Ok(0)
            }
            1 if self.is_controller && a as usize == self.tile.tasks.signatures.len() => {
                Ok(self.tile.tasks.wait_available(b))
            }
            1 if self.is_controller => Ok(self.tile.tasks.wait(a as usize)),
            2 if self.task_mode => {
                self.complete_task(a);
                Ok(0)
            }
            11 if self.task_mode || self.firmware_task.is_some() => Ok(0),
            2 if worker.is_some() && self.firmware_task.is_some() => {
                self.tile.tasks.complete(worker.unwrap(), a);
                self.firmware_task = None;
                Ok(0)
            }
            8 if worker.is_some() => {
                self.firmware_task = self.tile.tasks.take(worker.unwrap());
                Ok(1)
            }
            9 if worker.is_some() => {
                let task = self.firmware_task.as_ref().expect("worker has no task context");
                Ok(match a {
                    0 => task.entry, 1 => task.argument, 2 => task.stack, 3 => task.tls,
                    4 => task.workspace, 5 => task.gp,
                    6 => self.tile.tasks.signatures[worker.unwrap()],
                    7 => task.csrs.read(0x180, rvsim::Privilege::Machine, self.hart.id, 0, 0).unwrap(),
                    _ => return Err(Whatever::without_source("invalid task context field".into())),
                })
            }
            7 if self.is_controller => {
                self.tile.tasks.stage(a as usize, b).map_err(Whatever::without_source)?;
                Ok(0)
            }
            10 if self.is_controller => Ok(self.tile.tasks.poll(a as usize)),
            3 => Ok(self.tile.tasks.signatures.len() as u64),
            4 => Ok(self.tile.tasks.signatures[a as usize]),
            5 => Ok(self.tile.tasks.workspace(if self.tile.has_scheduler { worker } else { None })),
            6 if self.is_controller || !self.tile.has_scheduler => {
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
            self.elapsed_cycles += cycles;
            self.tile.clint.advance_to(self.hart.id as usize, self.elapsed_cycles);
            let inputs = crate::input::Inputs {
                cycles,
                hart: self.hart.id as usize,
                clint: &self.tile.clint,
                snapshot: std::cell::Cell::new(None),
            };
            let pc = self.hart.pc;
            let started = self.profile.then(Instant::now);
            let mut event = if let Some(request) = self.pending_control.take() {
                Step::Custom(request)
            } else { self.hart.step(
                &mut Port {
                    devices: &self.tile.platform,
                    memory: &self.tile.memory,
                },
                &self.mmu,
                inputs,
            ) };
            if let Some(started) = started {
                self.cpu_elapsed += started.elapsed();
            }
            self.waiting = event == Step::Waiting;
            if let Step::Custom(request) = event {
                if request.instruction & 0x7f == 0x2b {
                    let operation = request.instruction >> 25;
                    let worker_wait = operation == 8 && self.worker_index.is_some()
                        && !self.tile.tasks.has_pending(self.worker_index.unwrap());
                    let controller_wait = operation == 1 && self.is_controller &&
                        if request.rs1 as usize == self.tile.tasks.signatures.len() {
                            !self.tile.tasks.available(request.rs2)
                        } else {
                            self.tile.tasks.poll(request.rs1 as usize) == 0
                        };
                    if worker_wait || controller_wait {
                        self.pending_control = Some(request);
                        self.waiting = true;
                        break;
                    }
                }
                let value = match request.instruction & 0x7f {
                    0x7b => {
                        let mut translation = self.hart.translation_context();
                        if translation.privilege == rvsim::Privilege::Machine
                            && translation.satp != rvsim::mmu::Satp::Bare
                        {
                            translation.privilege = rvsim::Privilege::Supervisor;
                        }
                        let guest = GuestAccess {
                            platform: Port {
                                devices: &self.tile.platform,
                                memory: &self.tile.memory,
                            },
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
                    if self.task_mode && trap.cause == 8 && self.hart.register(17) == 0xbb01 {
                        self.complete_task(self.hart.register(10));
                        break;
                    }
                    if self.task_mode {
                        return Err(Whatever::without_source(format!(
                            "task trap at pc=0x{pc:x}: cause={} value=0x{:x} a7={} a0=0x{:x} a1=0x{:x} a2=0x{:x}",
                            trap.cause,
                            trap.value,
                            self.hart.register(17),
                            self.hart.register(10),
                            self.hart.register(11),
                            self.hart.register(12)
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
                Step::Waiting => (),
                Step::Custom(_) => unreachable!(),
            }
            let code = self.tile.exit_code.load(std::sync::atomic::Ordering::Acquire);
            if code != i64::MIN {
                self.exit_code = Some(code as i32);
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
    pub fn wait_control(&self) {
        if self.is_controller {
            if let Some(request) = self.pending_control {
                let core = request.rs1 as usize;
                if core == self.tile.tasks.signatures.len() {
                    self.tile.tasks.wait_available(request.rs2);
                } else {
                    self.tile.tasks.wait(core);
                }
            }
        }
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

#[cfg(test)]
mod identity_tests {
    use super::*;
    #[test]
    fn scu_exit_from_another_hart_is_global_and_preserves_first_code() {
        let topology = config::tile_topology(usize::from(config::tile_count() > 1));
        let core_index = topology.cores[0].1;
        let hart = config::core_hart_id(core_index);
        for code in [0_u32, 37] {
            let chip = Chip::new(4096, config::hart_capacity());
            let tile = Tile::new(&chip, &topology, Vec::new());
            let path = std::env::temp_dir().join(format!("bemu-scu-{}-{code}", std::process::id()));
            let mut core = Core::new_with_core_hart(&path, TraceConfig::new(false, false),
                false, false, core_index, hart, Some(Arc::clone(&tile))).unwrap();
            let target_hart = usize::from(hart == 0 && config::hart_capacity() > 1);
            let address = crate::root::platform::SCU_BASE
                + target_hart as u64 * crate::root::platform::SCU_STRIDE;
            let instructions = [
                address as u32 | 0x2b7,
                code << 20 | 0x313,
                0x0062a023,
                0x0000006f,
            ];
            let bytes: Vec<_> = instructions.into_iter().flat_map(u32::to_le_bytes).collect();
            write_guest(tile.memory.as_ref(), DRAM_BASE, &bytes).unwrap();
            core.hart.pc = DRAM_BASE;
            core.step(4).unwrap();
            assert_eq!(core.exit_code(), Some(code as i32));
            {
                use rvsim::bus::{Bus, Width};
                let mut platform = tile.platform.lock().unwrap();
                platform.write(address, Width::Word, (code ^ 1) as u64).unwrap();
            }
            assert_eq!(tile.exit_code.load(std::sync::atomic::Ordering::Acquire), code as i64);
            drop(core);
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    fn issue(core: &mut Core, funct: u8, rs1: u64, rs2: u64) {
        config::configure_core(core.core_index);
        let memory = GuestAccess { platform: Port { devices: &core.tile.platform, memory: &core.tile.memory },
            mmu: &core.mmu, translation: core.hart.translation_context() };
        accel::execute(&mut core.accel, memory, funct, rs1, rs2, 1);
        core.accel.finish_bank_frees();
    }

    #[test]
    fn shared_vbank_and_mvover_use_compute_indices() {
        let topology = config::tile_topology(usize::from(config::tile_count()>1));
        if topology.endpoint_cores.len()<2 { return; }
        let chip = Chip::new(1<<20,config::hart_capacity());
        let tile = Tile::new(&chip,&topology,Vec::new());
        let path = std::env::temp_dir().join(format!("bemu-mapping-{}",std::process::id()));
        let first=topology.endpoint_cores[0].1;let second=topology.endpoint_cores[1].1;
        let mut a=Core::new_with_core_hart(&path.join("a"),TraceConfig::new(false,false),false,false,
            first,config::core_hart_id(first),Some(Arc::clone(&tile))).unwrap();
        let mut b=Core::new_with_core_hart(&path.join("b"),TraceConfig::new(false,false),false,false,
            second,config::core_hart_id(second),Some(Arc::clone(&tile))).unwrap();
        config::configure_core(first);
        let shared=config::shared_vbank_base();
        if topology.shared_physical_bank_count>=2 {
            issue(&mut a,32,shared as u64,1057);issue(&mut b,32,shared as u64,1057);
            let state=tile.banks.lock().unwrap();
            let pa=state.map.resolve_hart_group(0,shared as u32,0).unwrap();
            let pb=state.map.resolve_hart_group(1,shared as u32,0).unwrap();
            assert_ne!(pa,pb);
            assert!(state.cfgs[shared].allocated);
            assert!(state.cfgs[state.virtual_bank_count+shared].allocated);
            drop(state);
            issue(&mut a,32,shared as u64,0);
            assert!(tile.banks.lock().unwrap().map.resolve_hart_group(1,shared as u32,0).is_some());
            issue(&mut b,32,shared as u64,0);
        }
        issue(&mut a,32,0,1057);issue(&mut b,32,0,1057);
        let bytes:Vec<u8>=(0..16).map(|i| i*7+1).collect();
        write_guest(tile.memory.as_ref(),DRAM_BASE+0x100,&bytes).unwrap();
        issue(&mut a,33,1<<30,(DRAM_BASE+0x100)|(1<<39));
        tile.mvover(1<<8,0).unwrap();
        issue(&mut b,16,1<<30,(DRAM_BASE+0x200)|(1<<39));
        let mut result=vec![0;16];tile.memory.read_buffer(DRAM_BASE+0x200,&mut result).unwrap();
        assert_eq!(bytes,result);
        let owner=tile.controller_hart.unwrap_or(tile.harts[0]);
        assert_eq!(tile.bank_owner_hart(a.hart_id(),true),owner);
        assert_eq!(tile.bank_owner_hart(b.hart_id(),false),b.hart_id());
        println!("MAPPING PASS controller={:?} cfg={first}/{second} physical={}/{} logical=0/1 capacity={} same-vbank mvover0->1 bytes=0diff",tile.controller_hart,a.hart_id(),b.hart_id(),config::hart_capacity());
        drop(a);drop(b);std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn bank_controller_and_cpu_worker_have_different_namespaces() {
        let mut topology=config::tile_topology(usize::from(config::tile_count()>1));
        let Some(cpu)=topology.cores.iter().map(|(_,core)| *core)
            .find(|core| !topology.endpoint_cores.iter().any(|(_,bb)| bb==core)) else { return; };
        if topology.endpoint_cores.len()<2 {return;}
        let gpu=topology.endpoint_cores[0].1;let other=topology.endpoint_cores[1].1;
        // Explicit role fixture: controller has banks, worker0 is CPU-only.
        topology.cores=vec![("controller".into(),gpu),("cpu".into(),cpu),("compute".into(),other)];
        topology.controller_core=Some(gpu);
        topology.endpoint_cores=vec![("controller".into(),gpu),("compute".into(),other)];
        topology.worker_cores=vec![("cpu".into(),cpu),("compute".into(),other)];
        let tile=Tile::new(&Chip::new(1<<20,config::hart_capacity()),&topology,
            vec![config::core_signature(cpu),config::core_signature(other)]);
        let path=std::env::temp_dir().join(format!("bemu-roles-{}",std::process::id()));
        let controller=Core::new_with_core_hart(&path.join("controller"),TraceConfig::new(false,false),false,false,gpu,config::core_hart_id(gpu),Some(Arc::clone(&tile))).unwrap();
        let mut worker=Core::new_with_core_hart(&path.join("cpu"),TraceConfig::new(false,false),false,false,cpu,config::core_hart_id(cpu),Some(Arc::clone(&tile))).unwrap();
        let bbworker=Core::new_with_core_hart(&path.join("compute"),TraceConfig::new(false,false),false,false,other,config::core_hart_id(other),Some(Arc::clone(&tile))).unwrap();
        assert!(controller.is_controller);assert_eq!(controller.worker_index,None);assert_eq!(controller.accel.endpoint_index,Some(0));
        assert!(!worker.is_controller);assert_eq!(worker.worker_index,Some(0));assert_eq!(worker.accel.endpoint_index,None);
        assert_eq!(bbworker.worker_index,Some(1));assert_eq!(bbworker.accel.endpoint_index,Some(1));
        assert_eq!(tile.private_endpoints.lock().unwrap().len(),2);
        tile.tasks.submit(0,config::core_signature(cpu),Task{entry:DRAM_BASE,argument:0,stack:DRAM_BASE+4096,tls:0,gp:0,workspace:1234,csrs:controller.hart.csrs.clone(),privilege:controller.hart.privilege}).unwrap();
        worker.start_task(tile.tasks.take(0).unwrap());worker.complete_task(0);
        assert_eq!(tile.tasks.poll(0),1);assert_eq!(tile.tasks.workspace(Some(0)),1234);
        println!("ROLE PASS BB controller endpoint0/no-worker; CPU worker0/no-endpoint; BB worker1/endpoint1");
        drop(controller);drop(worker);drop(bbworker);std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn actual_hart_and_logical_compute_namespaces_are_separate() {
        let tile_index = usize::from(config::tile_count() > 1);
        let topology = config::tile_topology(tile_index);
        let tile = Tile::new(&Chip::new(1 << 20, config::hart_capacity()), &topology, Vec::new());
        let core_index = topology.endpoint_cores[0].1;
        let hart = config::core_hart_id(core_index);
        println!("SIGNATURE cfg{core_index}={:016x}",config::core_signature(core_index));
        let path = std::env::temp_dir().join(format!("bemu-owner-{}", std::process::id()));
        let core = Core::new_with_core_hart(&path, TraceConfig::new(false,false),false,false,
            core_index,hart,Some(Arc::clone(&tile))).unwrap();
        assert_eq!(core.hart.id as usize, hart);
        assert_eq!(core.accel.hart_id, hart);
        assert_eq!(core.worker_index, tile.worker_harts.iter().position(|&id| id == hart));
        assert_eq!(tile.bank_owner_hart(hart,false),hart);
        let endpoints = tile.private_endpoints.lock().unwrap();
        assert!(endpoints.contains_key(&0));
        assert_eq!(endpoints.len(),1);
        drop(endpoints);drop(core);std::fs::remove_dir_all(path).unwrap();
    }
}
