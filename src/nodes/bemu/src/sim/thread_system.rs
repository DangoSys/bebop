use crate::{
    clint::Clint,
    configs,
    ddr::{Ddr, DRAM_BASE},
    guest::ThreadMemory,
    hlink::Peripherals,
    riscv::{
        bus::{Bus, BusError, HartBus, Width},
        registers::Registers,
    },
    sim::run::Args,
    tasks::Tasks,
    timing::{DeviceTime, DmaLayout, HartTime},
    tlink::TLink,
    DeviceModel, HartModel, ThreadGroup,
};
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// A thread alone has no hart to instantiate: `sim::run` runs its design.pb on [`run`].
pub fn instantiate(_: &DeviceModel, _: &Args) -> Result<Vec<Box<dyn HartModel>>, String> {
    Err("a thread alone has no harts; it runs on sim::thread_system".into())
}

/// The environment of a thread alone has no peripherals: its DMA reaches the TSS and the ideal memory only.
struct NoPeripherals(Arc<Ddr>);
impl Bus for NoPeripherals {
    fn read(&mut self, _: u64, _: Width) -> Result<u64, BusError> {
        Err(BusError)
    }
    fn write(&mut self, _: u64, _: Width, _: u64) -> Result<(), BusError> {
        Err(BusError)
    }
    fn peripheral(&mut self) -> bool {
        false
    }
    fn compare_exchange(&mut self, _: u64, _: Width, _: u64, _: u64) -> Result<u64, BusError> {
        Err(BusError)
    }
}
impl HartBus for NoPeripherals {
    fn load_reserved(&mut self, _: u64, _: u64, _: Width) -> Result<u64, BusError> {
        Err(BusError)
    }
    fn store_conditional(&mut self, _: u64, _: u64, _: Width, _: u64) -> Result<bool, BusError> {
        Err(BusError)
    }
}
impl Peripherals for NoPeripherals {
    fn memory(&self) -> &Ddr {
        &self.0
    }
    fn uart_log(&mut self, _: usize, _: std::fs::File) {
        panic!("a thread alone has no UART")
    }
    fn flush_uart(&mut self, _: usize) -> std::io::Result<()> {
        panic!("a thread alone has no UART")
    }
    fn capture_output(&mut self, _: Option<bebop_elf::OutputCapture>) {
        panic!("a thread alone has no output capture")
    }
}

/// The TSS window of the thread's DMA (arch: the StoreSlave of ThreadEnvironment): one row of the TSS a cycle.
struct TssWindow(Weak<TLink>);
impl TssWindow {
    fn offset(&self, address: u64, bytes: usize) -> Option<usize> {
        let link = self.0.upgrade().expect("thread link dropped");
        let (base, size) = link.tasks.tss_range.expect("thread alone has a TSS");
        (address >= base && address + bytes as u64 <= base + size as u64).then(|| (address - base) as usize)
    }
}
impl ThreadMemory for TssWindow {
    fn span(&self, _: usize, address: u64, _: bool) -> (bool, u64) {
        let link = self.0.upgrade().expect("thread link dropped");
        let (base, size) = link.tasks.tss_range.expect("thread alone has a TSS");
        if address < base {
            (false, base)
        } else if address < base + size as u64 {
            (true, base + size as u64)
        } else {
            (false, u64::MAX)
        }
    }
    fn contains(&self, _: usize, address: u64, bytes: usize, _: bool) -> bool {
        self.offset(address, bytes).is_some()
    }
    fn transfer_cycles(&self, _: usize, spans: &[(u64, usize)], _: bool) -> Option<u64> {
        Some(spans.iter().map(|(_, bytes)| (*bytes as u64).div_ceil(16)).sum())
    }
    fn start(&self, _: usize, _: u64) {}
    fn control(&self, _: u32, _: u64, _: u64) -> u64 {
        panic!("the TSS window has no control operations")
    }
    fn read(&self, _: usize, address: u64, output: &mut [u8]) -> bool {
        let Some(offset) = self.offset(address, output.len()) else {
            return false;
        };
        let link = self.0.upgrade().expect("thread link dropped");
        let tss = link.tasks.tss.as_ref().unwrap().lock().unwrap();
        output.copy_from_slice(&tss.bytes[offset..offset + output.len()]);
        true
    }
    fn write(&self, _: usize, address: u64, input: &[u8]) -> bool {
        let Some(offset) = self.offset(address, input.len()) else {
            return false;
        };
        let link = self.0.upgrade().expect("thread link dropped");
        let mut tss = link.tasks.tss.as_ref().unwrap().lock().unwrap();
        tss.bytes[offset..offset + input.len()].copy_from_slice(input);
        true
    }
}

/// The PT_LOAD segments of an ELF64 file, at their physical addresses, into the ideal memory.
fn load(elf: &std::path::Path, memory: &Ddr) -> Result<(), String> {
    let file = std::fs::read(elf).map_err(|error| format!("{}: {error}", elf.display()))?;
    let word = |at: usize, bytes: usize| {
        let mut value = [0; 8];
        value[..bytes].copy_from_slice(&file[at..at + bytes]);
        u64::from_le_bytes(value)
    };
    assert_eq!(&file[..5], b"\x7fELF\x02", "a thread program is an ELF64 file");
    let (table, size, count) = (word(32, 8) as usize, word(54, 2) as usize, word(56, 2) as usize);
    for header in (0..count).map(|index| table + index * size) {
        let (kind, offset, physical, bytes) = (
            word(header, 4),
            word(header + 8, 8),
            word(header + 24, 8),
            word(header + 32, 8),
        );
        if kind == 1 && bytes != 0 {
            memory
                .write_buffer(physical, &file[offset as usize..(offset + bytes) as usize])
                .map_err(|_| format!("segment at 0x{physical:x} is outside the ideal memory"))?;
        }
    }
    Ok(())
}

/// The thread simulation (arch: sims/thread/ThreadEnvironment): the thread of design.pb alone. Its host loads the
/// program image at the start of the ideal memory (a header of the code and TLS image bytes, then both) through the
/// TSS, launches one task and reports its completion value: the status in bits 31:0 (0 passes) and the cycles the
/// task measured (its mcycle), 0 when it measures none, in bits 63:32.
pub fn run(args: Args) -> Result<(), String> {
    let wall_start = std::time::Instant::now();
    let host = configs::alone().expect("a thread simulation runs a thread alone");
    assert_eq!(
        host.memory_base, DRAM_BASE,
        "the ideal memory of a thread alone is at the DDR base"
    );
    std::fs::create_dir_all(&args.log_dir).map_err(|error| error.to_string())?;
    let clint = Arc::new(Clint::new(1));
    clint.coordinate(vec![0], true);
    let memory = Arc::new(Ddr::new(host.memory_bytes as usize));
    let timing = Arc::new(DeviceTime::new(Arc::clone(&clint), 1));
    let link = Arc::new(TLink {
        hart_id: 0,
        clint: Arc::clone(&clint),
        platform: Arc::new(Mutex::new(NoPeripherals(Arc::clone(&memory)))),
        memory: Arc::clone(&memory),
        transfers: Mutex::new(HartTime::new(
            Arc::clone(&timing),
            DmaLayout {
                read_row_cycles: 1,
                range_maintenance: false,
                channel_by_source: vec![0, 0],
            },
        )),
        thread_memory: OnceLock::new(),
        tasks: Tasks::new(&[0], Arc::clone(&clint)),
    });
    assert!(link
        .thread_memory
        .set(Arc::new(TssWindow(Arc::downgrade(&link))))
        .is_ok());
    assert!(timing
        .threads
        .set(vec![Arc::clone(&link.tasks.contexts[0].progress)])
        .is_ok());
    assert!(timing.cpus.set(Vec::new()).is_ok());
    load(&args.elf, &memory)?;
    let mut group = ThreadGroup::new(
        &link,
        crate::thread_workers(&link, crate::ball::BUS, &args.model_options())?,
    )?;

    let owner = Registers::new(0, 0);
    let mut cycle = 0;
    let control = |operation, field: u64, data, cycle: &mut u64| {
        link.tasks.control(operation, field, data, &owner, &clint, cycle)
    };
    let read = |address| {
        let mut value = [0; 8];
        memory.read_buffer(address, &mut value).expect("thread image read");
        u64::from_le_bytes(value)
    };
    let (code_bytes, tls_bytes) = (read(DRAM_BASE), read(DRAM_BASE + 8));
    let tls = configs::ant_config(0).unwrap().tls.as_ref().unwrap();
    control(5, 0, 0, &mut cycle);
    // The code, then the TLS image, through the TSS into the thread's stores.
    for (operation, start, bytes) in [
        (8, DRAM_BASE + 16, code_bytes),
        (10, DRAM_BASE + 16 + code_bytes, tls_bytes),
    ] {
        if bytes == 0 {
            continue;
        }
        for offset in (0..bytes).step_by(8) {
            control(12, offset, read(start + offset), &mut cycle);
        }
        control(operation, 0, bytes << 32, &mut cycle);
    }
    for (field, value) in [
        1,
        0,
        code_bytes,
        tls.base,
        tls.base + tls.bytes as u64,
        configs::thread_signature(0),
    ]
    .into_iter()
    .enumerate()
    {
        control(1, field as u64, value, &mut cycle);
    }
    let start = cycle;
    control(2, 0, 0, &mut cycle);
    let event = control(0, 10, 0, &mut cycle);
    if event & 1 == 0 {
        group.shutdown()?;
        return Err(format!("thread task did not complete (event {event})"));
    }
    let value = control(0, 9, 0, &mut cycle);
    control(3, 0, 0, &mut cycle);
    control(6, 0, 0, &mut cycle);
    let finish = cycle;
    group.shutdown()?;
    let status = value as u32;
    // The status of the task and the cycles it took in the model.
    println!("Thread task: status {status}, {} cycles", finish - start);
    println!("BEMU wall time: {:.3}s", wall_start.elapsed().as_secs_f64());
    if status != 0 {
        return Err(format!("thread task failed with status {status}"));
    }
    Ok(())
}
