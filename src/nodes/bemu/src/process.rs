use crate::user_vm::UserVm;
use crate::root::memory::Pages;
use crate::root::platform::DRAM_BASE;
use bebop_dtb::DtbBuilder;
use bebop_elf::{LoadInfo, TlsInfo};
use bebop_syscall::{handle_syscall_with_state, set_guest_mappings, SyscallState};
use memory::Memory;
use rvsim::{hart::Hart, Privilege};
use std::os::unix::fs::FileExt;
use std::sync::{Arc, Mutex};

pub(crate) const PAGE_SIZE: u64 = 4096;
const USER_TOP: u64 = 0x40_0000_0000;
const USER_STACK_SIZE: u64 = 8 * 1024 * 1024;
const SYS_BRK: u64 = 214;
const SYS_MUNMAP: u64 = 215;
const SYS_MMAP: u64 = 222;

pub(crate) struct Process {
    pub(crate) streams: crate::stdio::Streams,
    pub(crate) syscall: SyscallState,
    pub(crate) program_name: String,
    pub(crate) arguments: Vec<String>,
    vm: Option<UserVm>,
    pub(crate) user_mode: bool,
}

impl Process {
    pub(crate) fn new() -> Self {
        Self {
            streams: crate::stdio::Streams::new(),
            syscall: SyscallState::new(),
            program_name: String::new(),
            arguments: Vec::new(),
            vm: None,
            user_mode: false,
        }
    }

    pub(crate) fn initialize(
        &mut self,
        hart: &mut Hart,
        memory: &dyn Memory,
        load: LoadInfo,
        pages: Arc<Mutex<Pages>>,
        image: (u64, u64),
    ) -> Result<(), String> {
        let user_mode = load.analysis.os_abi == bebop_elf::OsAbi::GnuUser;
        self.user_mode = user_mode;
        hart.privilege = Privilege::Machine;
        let mem_end = DRAM_BASE + memory.len() as u64;
        let working_dir = self.syscall.working_dir.clone();
        self.syscall = SyscallState::new();
        self.syscall.working_dir = working_dir;
        self.vm = None;
        set_guest_mappings(&[]);

        let brk_start = if user_mode {
            align_up(load.analysis.max_vaddr, PAGE_SIZE)
        } else {
            align_up(load.image_end, PAGE_SIZE)
        };
        let mmap_base = if user_mode {
            USER_TOP - USER_STACK_SIZE - PAGE_SIZE
        } else {
            align_down(mem_end - 8 * 1024 * 1024, PAGE_SIZE)
        };
        self.syscall.init_mem_layout(brk_start, mmap_base);
        if user_mode {
            self.syscall
                .set_mem_bounds(load.analysis.min_vaddr, USER_TOP - USER_STACK_SIZE);
        }

        let tp = if user_mode { None } else { setup_tls(memory, load.tls)? };

        hart.csrs
            .write(0x300, (3 << 13) | (3 << 9), Privilege::Machine)
            .unwrap();
        hart.csrs.write(0x306, 7, Privilege::Machine).unwrap();
        hart.csrs.write(0x106, 7, Privilege::Machine).unwrap();
        hart.csrs.pmp.set_address(0, (1 << 54) - 1);
        hart.csrs.pmp.set_config(0, 0x1f);
        if user_mode {
            let mut vm = setup_user_vm(memory, &load, pages, image)?;
            let regs = setup_user_stack(memory, &vm, &load, &self.program_name, &self.arguments)?;
            let (bootstrap, dtb) = install_user_bootstrap(memory, &mut vm, &load, regs)?;
            hart.pc = bootstrap;
            hart.set_register(10, hart.id);
            hart.set_register(11, dtb);
            self.vm = Some(vm);
        } else {
            let dtb_addr = install_dtb(memory)?;
            hart.pc = load.entry;
            hart.set_register(10, hart.id);
            hart.set_register(11, dtb_addr);
            if let Some(tp) = tp {
                hart.set_register(4, tp);
            }
        }
        Ok(())
    }

    pub(crate) fn syscall(
        &mut self,
        hart: &mut Hart,
        platform: &Mutex<crate::root::platform::Platform>,
    ) -> Result<(), String> {
        if let Some(vm) = &self.vm {
            set_guest_mappings(&vm.maps.iter().map(|m| (m.virt, m.phys, m.len)).collect::<Vec<_>>());
        } else {
            set_guest_mappings(&[]);
        }
        let old_brk = self.syscall.brk_addr;
        let old_mmap_regions = self.syscall.mmap_regions.clone();
        let old_mmap_base = self.syscall.mmap_base;
        let number = hart.register(17);
        let a0 = hart.register(10);
        let a1 = hart.register(11);
        let value = if let Some(value) = self
            .streams
            .syscall(number, a0, a1, hart.register(12) as usize, platform)
        {
            value
        } else {
            let platform = platform.lock().expect("BEMU platform poisoned");
            let memory: &dyn Memory = platform.memory.as_ref();
            let (mut value, _) = handle_syscall_with_state(
                &mut self.syscall,
                number,
                a0,
                a1,
                hart.register(12),
                hart.register(13),
                hart.register(14),
                hart.register(15),
                memory,
            );
            if let Some(vm) = &mut self.vm {
                if let Err(errno) = map_syscall_result(
                    memory,
                    vm,
                    &self.syscall,
                    &old_mmap_regions,
                    old_brk,
                    number,
                    [
                        a0,
                        a1,
                        hart.register(12),
                        hart.register(13),
                        hart.register(14),
                        hart.register(15),
                    ],
                    value,
                ) {
                    self.syscall.mmap_regions = old_mmap_regions;
                    self.syscall.mmap_base = old_mmap_base;
                    self.syscall.brk_addr = old_brk;
                    value = errno as u64;
                }
            }
            platform.memory.clear_reservations();
            value
        };
        hart.set_register(10, value);
        let epc = hart.csrs.read(0x341, Privilege::Machine, hart.id, 0, 0).unwrap();
        hart.csrs.write(0x341, epc.wrapping_add(4), Privilege::Machine).unwrap();
        hart.return_from_machine_trap();
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn map_syscall_result(
    memory: &dyn Memory,
    user_vm: &mut UserVm,
    state: &SyscallState,
    old_regions: &[(u64, u64, u64)],
    old_brk: u64,
    syscall_num: u64,
    args: [u64; 6],
    result: u64,
) -> Result<(), i64> {
    if (result as i64) < 0 {
        return Ok(());
    }
    let [a0, a1, prot, flags, fd, offset] = args;
    match syscall_num {
        SYS_BRK if result > old_brk => {
            let start = align_up(old_brk, PAGE_SIZE);
            let end = align_up(result, PAGE_SIZE);
            if end > start {
                user_vm
                    .alloc_user_pages(memory, start, end - start, 0x2 | 0x4)
                    .map_err(|_| -12)?;
            }
        }
        SYS_MMAP => {
            let len = align_up(a1, PAGE_SIZE);
            // PROT_NONE reserves virtual space without allocating physical DDR pages.
            if prot == 0 {
                if flags & 0x10 != 0 {
                    user_vm.free_user_pages(memory, result, len).map_err(|_| -12)?;
                }
                return Ok(());
            }
            let pte_flags = (if prot & 3 != 0 { 0x2 } else { 0 }) | ((prot & 2) << 1) | ((prot & 4) << 1);
            let backed_len = if flags & 0x20 == 0 {
                let metadata = state
                    .open_files
                    .get(&fd)
                    .ok_or(-9i64)?
                    .metadata()
                    .map_err(|error| -(error.raw_os_error().unwrap_or(5) as i64))?;
                len.min(align_up(metadata.len().saturating_sub(offset), PAGE_SIZE))
            } else {
                len
            };
            if backed_len == 0 {
                return Ok(());
            }
            let phys = user_vm
                .alloc_user_pages(memory, result, backed_len, pte_flags)
                .map_err(|_| -12)?;
            if flags & 0x20 == 0 {
                let file = state.open_files.get(&fd).ok_or(-9i64)?;
                let mut buffer = [0u8; 65536];
                let mut cursor = 0;
                while cursor < backed_len {
                    let bytes = buffer.len().min((backed_len - cursor) as usize);
                    let count = match file.read_at(&mut buffer[..bytes], offset + cursor) {
                        Ok(count) => count,
                        Err(error) => {
                            user_vm.free_user_pages(memory, result, len).map_err(|_| -12)?;
                            return Err(-(error.raw_os_error().unwrap_or(5) as i64));
                        }
                    };
                    if count == 0 {
                        break;
                    }
                    write_guest(memory, phys + cursor, &buffer[..count]).map_err(|_| -14)?;
                    cursor += count as u64;
                }
            }
        }
        SYS_MUNMAP => {
            let end = a0 + align_up(a1, PAGE_SIZE);
            for &(base, bytes, _) in old_regions {
                let start = a0.max(base);
                let stop = end.min(base + bytes);
                if start < stop {
                    user_vm.free_user_pages(memory, start, stop - start).map_err(|_| -12)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

struct InitialRegs {
    sp: u64,
    a0: u64,
    a1: u64,
    a2: u64,
}

fn setup_tls(memory: &dyn Memory, tls: Option<TlsInfo>) -> Result<Option<u64>, String> {
    let Some(tls) = tls else {
        return Ok(None);
    };

    let align = tls.align.max(16);
    let tls_size = align_up(tls.memsz, align);
    let total_size = tls_size + align;
    let tls_area_addr = DRAM_BASE + memory.len() as u64 - total_size - 0x10000;
    let tp = align_up(tls_area_addr, align);
    if tls.filesz > tls.memsz {
        return Err("TLS file size exceeds its memory size".to_string());
    }
    let copy_size = tls.filesz as usize;

    if copy_size > 0 {
        let src_offset = guest_offset(memory, tls.vaddr)?;
        let dst_offset = guest_offset(memory, tp)?;
        if src_offset + copy_size > memory.len() || dst_offset + copy_size > memory.len() {
            return Err(format!("TLS copy exceeds memory: size={copy_size}"));
        }
        let mut src = vec![0; copy_size];
        memory.read_buffer(src_offset, &mut src);
        memory.write_buffer(dst_offset, &src);
    }

    if tls.memsz > tls.filesz {
        let bss_start = tp + tls.filesz;
        let bss_offset = guest_offset(memory, bss_start)?;
        let bss_size = (tls.memsz - tls.filesz) as usize;
        if bss_offset + bss_size > memory.len() {
            return Err(format!("TLS BSS exceeds memory: addr=0x{bss_start:x} size={bss_size}"));
        }
        memory.fill(bss_offset, bss_size, 0);
    }

    Ok(Some(tp))
}

fn install_dtb(memory: &dyn Memory) -> Result<u64, String> {
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, memory.len() as u64, None, None);
    let mem_end = DRAM_BASE + memory.len() as u64;
    let dtb_addr = align_down(mem_end - 0x20_0000 - dtb.len() as u64, PAGE_SIZE);
    write_guest(memory, dtb_addr, &dtb)?;
    Ok(dtb_addr)
}

fn install_user_bootstrap(
    memory: &dyn Memory,
    vm: &mut UserVm,
    load: &LoadInfo,
    regs: InitialRegs,
) -> Result<(u64, u64), String> {
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, memory.len() as u64, None, None);
    let boot = vm.allocate_physical(128 + dtb.len() as u64)?;
    // AUIPC t0 points to the physical descriptor; paging applies only after MRET.
    let mut code = vec![0x0000_0297u32];
    let ld = |rd: u32, offset: u32| (offset << 20) | (5 << 15) | (3 << 12) | (rd << 7) | 0x03;
    let csrw = |csr: u32| (csr << 20) | (6 << 15) | (1 << 12) | 0x73;
    code.extend([ld(6, 64), csrw(0x180), 0x1200_0073]);
    code.extend([ld(6, 72), csrw(0x341), ld(6, 80), csrw(0x300)]);
    code.extend([ld(2, 88), ld(10, 96), ld(11, 104), ld(12, 112), 0x3020_0073]);
    let mut bytes = Vec::new();
    for instruction in code { bytes.extend_from_slice(&instruction.to_le_bytes()); }
    bytes.resize(64, 0);
    for value in [vm.satp(), load.analysis.original_entry, (3 << 13) | (3 << 9),
                  regs.sp, regs.a0, regs.a1, regs.a2] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.resize(128, 0);
    bytes.extend_from_slice(&dtb);
    write_guest(memory, boot, &bytes)?;
    Ok((boot, boot + 128))
}

fn setup_user_vm(
    memory: &dyn Memory,
    load: &LoadInfo,
    pages: Arc<Mutex<Pages>>,
    image: (u64, u64),
) -> Result<UserVm, String> {
    let stack_virt_bottom = USER_TOP - USER_STACK_SIZE;
    let mut vm = UserVm::new(memory, pages, image)?;

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
        vm.map_range(memory, seg.vaddr, phys + image.0 - DRAM_BASE, seg.memsz, flags)?;
    }

    vm.alloc_user_pages(memory, stack_virt_bottom, USER_STACK_SIZE, 0x2 | 0x4)?;
    let maps: Vec<(u64, u64, u64)> = vm.maps.iter().map(|m| (m.virt, m.phys, m.len)).collect();
    set_guest_mappings(&maps);
    Ok(vm)
}

fn setup_user_stack(
    memory: &dyn Memory,
    vm: &UserVm,
    load: &LoadInfo,
    program_name: &str,
    arguments: &[String],
) -> Result<InitialRegs, String> {
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
    let mut cursor = stack_top;
    let mut argv = Vec::with_capacity(arguments.len() + 1);
    for argument in std::iter::once(program_name).chain(arguments.iter().map(String::as_str)) {
        let mut bytes = argument.as_bytes().to_vec();
        bytes.push(0);
        cursor -= bytes.len() as u64;
        vm.write_user(memory, cursor, &bytes)?;
        argv.push(cursor);
    }
    let string_addr = argv[0];
    let random_len = 16u64;
    let word_size = 8u64;
    let random_addr = align_down(cursor - random_len, 16);
    let phdr_addr = user_image_addr(load, load.program_headers.addr)?;

    let mut stack_entries = Vec::with_capacity(40 + argv.len());
    stack_entries.push(argv.len() as u64);
    stack_entries.extend(argv);
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

pub(crate) fn write_guest(memory: &dyn Memory, addr: u64, bytes: &[u8]) -> Result<(), String> {
    let offset = guest_offset(memory, addr)?;
    let end = offset + bytes.len();
    if end > memory.len() {
        return Err(format!(
            "guest write exceeds memory: addr=0x{addr:x} size={}",
            bytes.len()
        ));
    }
    memory.write_buffer(offset, bytes);
    Ok(())
}

pub(crate) fn guest_offset(memory: &dyn Memory, addr: u64) -> Result<usize, String> {
    if addr < DRAM_BASE {
        return Err(format!("guest address below DRAM: 0x{addr:x}"));
    }
    let offset = (addr - DRAM_BASE) as usize;
    if offset >= memory.len() {
        return Err(format!("guest address outside memory: 0x{addr:x}"));
    }
    Ok(offset)
}

pub(crate) fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

pub(crate) fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}
