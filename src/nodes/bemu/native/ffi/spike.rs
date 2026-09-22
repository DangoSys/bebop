use super::callbacks::{
    bemu_take_barrier, spike_create_raw, spike_destroy_raw, spike_exit_code_raw, spike_finished_raw,
    spike_init_hart_raw, spike_step_elapsed_ns_raw, spike_step_raw, spike_stop_raw,
};
use super::pk::PkVm;
use super::state::{EmuState, SharedMemory};
use super::*;

pub struct NativeSpike {
    ctx: *mut c_void,
    state: Box<EmuState>,
    loaded_elf: Option<LoadInfo>,
}

unsafe impl Send for NativeSpike {}

impl NativeSpike {
    pub fn load_elf(&mut self, elf_path: &str) -> Result<(), String> {
        self.loaded_elf = Some(load_elf_memory(&mut self.state, elf_path)?);
        Ok(())
    }

    pub fn init_hart(&mut self, pk: bool) -> Result<(), String> {
        let load = self
            .loaded_elf
            .take()
            .ok_or_else(|| "cannot initialize hart before loading ELF".to_string())?;
        hart_init(self.ctx, &mut self.state, load, pk)
    }

    pub fn step(&mut self, count: u64) -> Result<(), String> {
        let ret = unsafe { spike_step_raw(self.ctx, count) };
        if ret < 0 {
            Err(format!("spike step failed with code {}", self.exit_code()))
        } else {
            Ok(())
        }
    }

    pub fn take_barrier(&mut self) -> bool {
        bemu_take_barrier(self.state_ptr())
    }

    fn state_ptr(&self) -> *mut c_void {
        self.state.as_ref() as *const EmuState as *mut c_void
    }

    pub fn finished(&self) -> bool {
        unsafe { spike_finished_raw(self.ctx) }
    }

    pub fn exit_code(&self) -> i32 {
        unsafe { spike_exit_code_raw(self.ctx) }
    }

    pub fn stop(&mut self, code: i32) {
        unsafe { spike_stop_raw(self.ctx, code) }
    }

    pub fn total_latency(&self) -> u64 {
        self.state.total_lat
    }

    pub fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        let spike_step = Duration::from_nanos(unsafe { spike_step_elapsed_ns_raw(self.ctx) });
        self.state.profile.report(total, spike_step)
    }
}

impl Drop for NativeSpike {
    fn drop(&mut self) {
        unsafe { spike_destroy_raw(self.ctx) };
    }
}

pub fn create_spike(
    isa: &str,
    hart_id: usize,
    shared_memory: Option<Arc<SharedMemory>>,
    log_path: Option<&str>,
    log_dir: &Path,
    trace_config: TraceConfig,
    profile: bool,
) -> Result<NativeSpike, String> {
    use std::ffi::CString;

    std::fs::create_dir_all(log_dir)
        .map_err(|e| format!("failed to create BEMU log dir {}: {e}", log_dir.display()))?;
    let isa_c = CString::new(isa).map_err(|e| e.to_string())?;
    let log_c = log_path.map(CString::new).transpose().map_err(|e| e.to_string())?;
    let mut state = Box::new(EmuState::new(log_dir, trace_config, profile, hart_id, shared_memory)?);
    let mem_ptr = state.memory.as_mut_ptr();
    let mem_size = state.memory.len();
    let uart_ptr = &mut state.uart as *mut Uart as *mut u8;
    let clint_ptr = &mut state.clint as *mut Clint as *mut u8;
    let plic_ptr = &mut state.plic as *mut Plic as *mut u8;
    let state_ptr = &mut *state as *mut EmuState as *mut c_void;

    let ctx = unsafe {
        spike_create_raw(
            isa_c.as_ptr(),
            1,
            hart_id,
            mem_ptr,
            mem_size,
            log_c.as_ref().map_or(std::ptr::null(), |path| path.as_ptr()),
            uart_ptr,
            clint_ptr,
            plic_ptr,
            state_ptr,
            profile,
        )
    };
    if ctx.is_null() {
        Err("failed to create spike instance".to_string())
    } else {
        Ok(NativeSpike {
            ctx,
            state,
            loaded_elf: None,
        })
    }
}

struct HartInit {
    entry: u64,
    trap_handler_addr: u64,
    satp: u64,
    regs: InitialRegs,
    tp: Option<u64>,
    pk: bool,
}

fn load_elf_memory(state: &mut EmuState, elf_path: &str) -> Result<LoadInfo, String> {
    state.syscall.working_dir = Path::new(elf_path)
        .parent()
        .expect("BEMU ELF must have a parent directory")
        .to_path_buf();
    let load = load_elf(elf_path, &mut state.memory, DRAM_BASE)?;
    let entry = load.entry;
    let mem_end = DRAM_BASE + state.memory.len() as u64;

    if entry < DRAM_BASE || entry >= mem_end {
        return Err(format!(
            "ELF entry outside BEMU DRAM: original=0x{:x} entry=0x{:x} valid=0x{:x}..0x{:x}",
            load.analysis.original_entry, entry, DRAM_BASE, mem_end
        ));
    }

    if load.analysis.needs_relocation {
        eprintln!(
            "[INFO] relocated ELF: entry 0x{:x} -> 0x{:x}, image 0x{:x}..0x{:x} -> end 0x{:x}",
            load.analysis.original_entry,
            load.analysis.entry,
            load.analysis.min_vaddr,
            load.analysis.max_vaddr,
            load.analysis.image_end
        );
    }

    Ok(load)
}

fn hart_init(ctx: *mut c_void, state: &mut EmuState, load: LoadInfo, pk: bool) -> Result<(), String> {
    let mem_end = DRAM_BASE + state.memory.len() as u64;
    let working_dir = state.syscall.working_dir.clone();
    state.syscall = SyscallState::new();
    state.syscall.working_dir = working_dir;
    state.pk_vm = None;
    set_guest_mappings(&[]);

    let brk_start = if pk {
        align_up(load.analysis.max_vaddr, PAGE_SIZE)
    } else {
        align_up(load.image_end, PAGE_SIZE)
    };
    let mmap_base = if pk {
        align_down(PK_MMAP_CEILING, PAGE_SIZE)
    } else {
        align_down(mem_end - 8 * 1024 * 1024, PAGE_SIZE)
    };
    state.syscall.init_mem_layout(brk_start, mmap_base);
    if pk {
        state
            .syscall
            .set_mem_bounds(load.analysis.min_vaddr, USER_TOP - USER_STACK_SIZE);
    }

    let tp = if pk {
        None
    } else {
        setup_tls(&mut state.memory, load.tls)?
    };
    let dtb_addr = install_dtb(&mut state.memory)?;

    let trap_handler_addr = if pk {
        install_pk_trap_handler(&mut state.memory)?
    } else {
        0
    };
    let (entry, satp, initial_regs) = if pk {
        let pk_vm = setup_pk_vm(&mut state.memory, &load)?;
        let regs = setup_pk_stack(&mut state.memory, &pk_vm, &load)?;
        let satp = pk_vm.satp();
        state.pk_vm = Some(pk_vm);
        (load.analysis.original_entry, satp, regs)
    } else {
        (
            load.entry,
            0,
            InitialRegs {
                sp: 0,
                a0: 0,
                a1: dtb_addr,
                a2: 0,
            },
        )
    };

    let hart = HartInit {
        entry,
        trap_handler_addr,
        satp,
        regs: initial_regs,
        tp,
        pk,
    };
    let tp_ptr = hart.tp.as_ref().map(|v| v as *const u64).unwrap_or(std::ptr::null());

    let initialized = unsafe {
        spike_init_hart_raw(
            ctx,
            hart.entry,
            hart.trap_handler_addr,
            hart.satp,
            hart.regs.sp,
            hart.regs.a0,
            hart.regs.a1,
            hart.regs.a2,
            tp_ptr,
            hart.pk,
        )
    };
    if initialized {
        Ok(())
    } else {
        Err("failed to initialize Spike hart state".to_string())
    }
}

struct InitialRegs {
    sp: u64,
    a0: u64,
    a1: u64,
    a2: u64,
}

fn setup_tls(memory: &mut [u8], tls: Option<TlsInfo>) -> Result<Option<u64>, String> {
    let Some(tls) = tls else {
        return Ok(None);
    };

    let align = tls.align.max(16);
    let tls_size = align_up(tls.memsz, align);
    let total_size = tls_size + align;
    let tls_area_addr = DRAM_BASE + memory.len() as u64 - total_size - 0x10000;
    let tp = align_up(tls_area_addr, align);
    let copy_size = tls.filesz.min(tls.memsz) as usize;

    if copy_size > 0 {
        let src_offset = guest_offset(memory, tls.vaddr)?;
        let dst_offset = guest_offset(memory, tp)?;
        let src = memory
            .get(src_offset..src_offset + copy_size)
            .ok_or_else(|| format!("TLS source exceeds memory: addr=0x{:x} size={copy_size}", tls.vaddr))?
            .to_vec();
        let dst = memory
            .get_mut(dst_offset..dst_offset + copy_size)
            .ok_or_else(|| format!("TLS destination exceeds memory: addr=0x{tp:x} size={copy_size}"))?;
        dst.copy_from_slice(&src);
    }

    if tls.memsz > tls.filesz {
        let bss_start = tp + tls.filesz;
        let bss_offset = guest_offset(memory, bss_start)?;
        let bss_size = (tls.memsz - tls.filesz) as usize;
        memory
            .get_mut(bss_offset..bss_offset + bss_size)
            .ok_or_else(|| format!("TLS BSS exceeds memory: addr=0x{bss_start:x} size={bss_size}"))?
            .fill(0);
    }

    write_guest(memory, tp, &tp.to_le_bytes())?;
    Ok(Some(tp))
}

fn install_dtb(memory: &mut [u8]) -> Result<u64, String> {
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, memory.len() as u64, None, None);
    let mem_end = DRAM_BASE + memory.len() as u64;
    let dtb_addr = align_down(mem_end - 0x20_0000 - dtb.len() as u64, PAGE_SIZE);
    write_guest(memory, dtb_addr, &dtb)?;
    Ok(dtb_addr)
}

fn install_pk_trap_handler(memory: &mut [u8]) -> Result<u64, String> {
    let trap_handler_addr = DRAM_BASE + memory.len() as u64 - 0x2000;
    let syscall_magic_addr = DRAM_BASE + memory.len() as u64 - 0x1000;
    let offset = syscall_magic_addr as i64 - trap_handler_addr as i64;
    let imm20 = ((offset >> 20) as u32) & 0x1;
    let imm10_1 = ((offset >> 1) as u32) & 0x3ff;
    let imm11 = ((offset >> 11) as u32) & 0x1;
    let imm19_12 = ((offset >> 12) as u32) & 0xff;
    let jal = 0x6f | (imm19_12 << 12) | (imm11 << 20) | (imm10_1 << 21) | (imm20 << 31);
    write_guest(memory, trap_handler_addr, &jal.to_le_bytes())?;
    Ok(trap_handler_addr)
}

fn setup_pk_vm(memory: &mut [u8], load: &LoadInfo) -> Result<PkVm, String> {
    let mem_end = DRAM_BASE + memory.len() as u64;
    let pt_root = align_down(mem_end - PK_HIGH_RESERVE, PAGE_SIZE);
    let pt_end = pt_root + PK_PT_RESERVE;
    let stack_phys_bottom = align_down(pt_root - USER_STACK_SIZE, PAGE_SIZE);
    let stack_virt_bottom = USER_TOP - USER_STACK_SIZE;
    let next_page = align_up(load.image_end, PAGE_SIZE);
    let mut vm = PkVm::new(memory, pt_root, pt_end, next_page, stack_phys_bottom)?;

    for seg in &load.analysis.load_segments {
        let phys = if load.analysis.is_pie || load.analysis.needs_relocation {
            DRAM_BASE + (seg.vaddr - load.analysis.min_vaddr)
        } else {
            seg.vaddr
        };
        let mut flags = 0;
        if (seg.flags & 0x4) != 0 {
            flags |= 0x2;
        }
        if (seg.flags & 0x2) != 0 {
            flags |= 0x4;
        }
        if (seg.flags & 0x1) != 0 {
            flags |= 0x8;
        }
        vm.map_range(memory, seg.vaddr, phys, seg.memsz, flags)?;
    }

    vm.map_range(memory, stack_virt_bottom, stack_phys_bottom, USER_STACK_SIZE, 0x2 | 0x4)?;
    let maps: Vec<(u64, u64, u64)> = vm.maps.iter().map(|m| (m.virt, m.phys, m.len)).collect();
    set_guest_mappings(&maps);
    Ok(vm)
}

fn setup_pk_stack(memory: &mut [u8], vm: &PkVm, load: &LoadInfo) -> Result<InitialRegs, String> {
    const AT_NULL: u64 = 0;
    const AT_PHDR: u64 = 3;
    const AT_PHENT: u64 = 4;
    const AT_PHNUM: u64 = 5;
    const AT_PAGESZ: u64 = 6;
    const AT_BASE: u64 = 7;
    const AT_ENTRY: u64 = 9;
    const AT_UID: u64 = 11;
    const AT_EUID: u64 = 12;
    const AT_GID: u64 = 13;
    const AT_EGID: u64 = 14;
    const AT_HWCAP: u64 = 16;
    const AT_SECURE: u64 = 23;
    const AT_RANDOM: u64 = 25;
    const AT_HWCAP2: u64 = 26;
    const AT_EXECFN: u64 = 31;

    let stack_top = align_down(USER_TOP - 16, 16);
    let prog_name = b"tutorial-linux\0";
    let random_len = 16u64;
    let word_size = 8u64;

    let string_addr = align_down(stack_top - prog_name.len() as u64, 16);
    let random_addr = align_down(string_addr - random_len, 16);
    let phdr_addr = user_image_addr(load, load.program_headers.addr)?;

    let mut stack_entries = Vec::with_capacity(40);
    stack_entries.push(1);
    stack_entries.push(string_addr);
    stack_entries.push(0);
    stack_entries.push(0);
    stack_entries.extend_from_slice(&[
        AT_PHDR,
        phdr_addr,
        AT_PHENT,
        load.program_headers.entry_size,
        AT_PHNUM,
        load.program_headers.count,
        AT_PAGESZ,
        PAGE_SIZE,
        AT_BASE,
        0,
        AT_HWCAP,
        0,
        AT_ENTRY,
        load.analysis.original_entry,
        AT_UID,
        0,
        AT_EUID,
        0,
        AT_GID,
        0,
        AT_EGID,
        0,
        AT_SECURE,
        0,
        AT_RANDOM,
        random_addr,
        AT_HWCAP2,
        0,
        AT_EXECFN,
        string_addr,
        AT_NULL,
        0,
    ]);

    let sp = align_down(random_addr - stack_entries.len() as u64 * word_size, 16);
    vm.write_user(memory, string_addr, prog_name)?;
    for i in 0..random_len {
        vm.write_user(memory, random_addr + i, &[0xA5u8 ^ i as u8])?;
    }
    for (i, value) in stack_entries.iter().enumerate() {
        vm.write_user(memory, sp + i as u64 * word_size, &value.to_le_bytes())?;
    }

    Ok(InitialRegs {
        sp,
        a0: 0,
        a1: 0,
        a2: 0,
    })
}

fn user_image_addr(load: &LoadInfo, phys_addr: u64) -> Result<u64, String> {
    if !load.analysis.is_pie && !load.analysis.needs_relocation {
        return Ok(phys_addr);
    }
    if phys_addr < DRAM_BASE || phys_addr > load.image_end {
        return Err(format!("loaded image address outside relocated image: 0x{phys_addr:x}"));
    }
    Ok(load.analysis.min_vaddr + (phys_addr - DRAM_BASE))
}

pub(super) fn write_guest(memory: &mut [u8], addr: u64, bytes: &[u8]) -> Result<(), String> {
    let offset = guest_offset(memory, addr)?;
    let end = offset + bytes.len();
    if end > memory.len() {
        return Err(format!(
            "guest write exceeds memory: addr=0x{addr:x} size={}",
            bytes.len()
        ));
    }
    memory[offset..end].copy_from_slice(bytes);
    Ok(())
}

pub(super) fn guest_offset(memory: &[u8], addr: u64) -> Result<usize, String> {
    if addr < DRAM_BASE {
        return Err(format!("guest address below DRAM: 0x{addr:x}"));
    }
    let offset = (addr - DRAM_BASE) as usize;
    if offset >= memory.len() {
        return Err(format!("guest address outside memory: 0x{addr:x}"));
    }
    Ok(offset)
}

pub(super) fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

pub(super) fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}
