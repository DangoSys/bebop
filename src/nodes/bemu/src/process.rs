use crate::pk::PkVm;
use crate::root::platform::DRAM_BASE;
use bebop_dtb::DtbBuilder;
use bebop_elf::{LoadInfo, TlsInfo};
use bebop_syscall::{handle_syscall_with_state, set_guest_mappings, SyscallState};
use rvsim::{hart::Hart, Privilege};
use crate::root::memory::Pages;
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
    vm: Option<PkVm>,
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
        memory: &mut [u8],
        load: LoadInfo,
        pk: bool,
        pages: Arc<Mutex<Pages>>,
        image: (u64, u64),
    ) -> Result<(), String> {
        self.user_mode = pk;
        let mem_end = DRAM_BASE + memory.len() as u64;
        let working_dir = self.syscall.working_dir.clone();
        self.syscall = SyscallState::new();
        self.syscall.working_dir = working_dir;
        self.vm = None;
        set_guest_mappings(&[]);

        let brk_start = if pk {
            align_up(load.analysis.max_vaddr, PAGE_SIZE)
        } else {
            align_up(load.image_end, PAGE_SIZE)
        };
        let mmap_base = if pk {
            USER_TOP - USER_STACK_SIZE - PAGE_SIZE
        } else {
            align_down(mem_end - 8 * 1024 * 1024, PAGE_SIZE)
        };
        self.syscall.init_mem_layout(brk_start, mmap_base);
        if pk {
            self.syscall
                .set_mem_bounds(load.analysis.min_vaddr, USER_TOP - USER_STACK_SIZE);
        }

        let tp = if pk { None } else { setup_tls(memory, load.tls)? };

        hart.csrs
            .write(0x300, (3 << 13) | (3 << 9), Privilege::Machine)
            .unwrap();
        hart.csrs.write(0x306, 7, Privilege::Machine).unwrap();
        hart.csrs.write(0x106, 7, Privilege::Machine).unwrap();
        hart.csrs.pmp.set_address(0, (1 << 54) - 1);
        hart.csrs.pmp.set_config(0, 0x1f);
        if pk {
            let vm = setup_pk_vm(memory, &load, pages, image)?;
            let regs = setup_pk_stack(memory, &vm, &load, &self.program_name, &self.arguments)?;
            hart.csrs.satp.write(vm.satp());
            hart.pc = load.analysis.original_entry;
            hart.privilege = Privilege::User;
            hart.set_register(2, regs.sp);
            hart.set_register(10, regs.a0);
            hart.set_register(11, regs.a1);
            hart.set_register(12, regs.a2);
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

    pub(crate) fn syscall(&mut self, hart: &mut Hart, platform: &Mutex<crate::root::platform::Platform>) -> Result<(), String> {
        if let Some(vm) = &self.vm {
            set_guest_mappings(&vm.maps.iter().map(|m| (m.virt, m.phys, m.len)).collect::<Vec<_>>());
        } else {
            set_guest_mappings(&[]);
        }
        let old_brk = self.syscall.brk_addr;
        let number = hart.register(17);
        let a0 = hart.register(10);
        let a1 = hart.register(11);
        let value = if let Some(value) = self.streams.syscall(number, a0, a1, hart.register(12) as usize, platform) {
            value
        } else {
        let mut platform = platform.lock().expect("BEMU platform poisoned");
        let memory = &mut platform.memory;
        let (value, _) = handle_syscall_with_state(
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
            map_syscall_result(memory, vm, old_brk, number, a0, a1, value)?;
        }
        value
        };
        hart.set_register(10, value);
        let epc = hart.csrs.read(0x341, Privilege::Machine, hart.id, 0, 0).unwrap();
        hart.csrs.write(0x341, epc.wrapping_add(4), Privilege::Machine).unwrap();
        hart.return_from_machine_trap();
        Ok(())
    }
}

fn map_syscall_result(
    memory: &mut [u8],
    pk_vm: &mut PkVm,
    old_brk: u64,
    syscall_num: u64,
    a0: u64,
    a1: u64,
    result: u64,
) -> Result<(), String> {
    if (result as i64) < 0 {
        return Ok(());
    }

    match syscall_num {
        SYS_BRK if result > old_brk => {
            let start = align_up(old_brk, PAGE_SIZE);
            let end = align_up(result, PAGE_SIZE);
            if end > start {
                pk_vm.alloc_user_pages(memory, start, end - start, 0x2 | 0x4)?;
            }
        }
        SYS_MMAP => {
            let len = align_up(a1, PAGE_SIZE);
            if result != 0 && len != 0 {
                pk_vm.alloc_user_pages(memory, result, len, 0x2 | 0x4)?;
            }
        }
        SYS_MUNMAP => pk_vm.free_user_pages(memory, a0, a1)?,
        _ => {
            let _ = a0;
        }
    }
    Ok(())
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
    if tls.filesz > tls.memsz {
        return Err("TLS file size exceeds its memory size".to_string());
    }
    let copy_size = tls.filesz as usize;

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

    Ok(Some(tp))
}

fn install_dtb(memory: &mut [u8]) -> Result<u64, String> {
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, memory.len() as u64, None, None);
    let mem_end = DRAM_BASE + memory.len() as u64;
    let dtb_addr = align_down(mem_end - 0x20_0000 - dtb.len() as u64, PAGE_SIZE);
    write_guest(memory, dtb_addr, &dtb)?;
    Ok(dtb_addr)
}

fn setup_pk_vm(memory: &mut [u8], load: &LoadInfo, pages: Arc<Mutex<Pages>>, image: (u64, u64)) -> Result<PkVm, String> {
    let stack_virt_bottom = USER_TOP - USER_STACK_SIZE;
    let interconnect = pages.lock().expect("DDR page pool poisoned").interconnect_buffer;
    let mut vm = PkVm::new(memory, pages, image)?;
    if let Some((physical, bytes)) = interconnect {
        use crate::root::interconnect::port::{BASE, SIZE, BUFFER_BASE};
        vm.map_range(memory, BASE, BASE, SIZE, 0x2 | 0x4)?;
        vm.map_range(memory, BUFFER_BASE, physical, bytes, 0x2 | 0x4)?;
    }

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

fn setup_pk_stack(
    memory: &mut [u8],
    vm: &PkVm,
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

pub(crate) fn write_guest(memory: &mut [u8], addr: u64, bytes: &[u8]) -> Result<(), String> {
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

pub(crate) fn guest_offset(memory: &[u8], addr: u64) -> Result<usize, String> {
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
