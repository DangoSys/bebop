use bebop_clint::Clint;
use bebop_plic::Plic;
use bebop_uart::CycleTraceCollector;
use rvsim::bus::{Bus, BusError, HartBus, Width};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufWriter, Write},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
};

pub const DRAM_BASE: u64 = 0x8000_0000;
pub const DTB_ADDRESS: u64 = 0x9000_0000;
pub const INITRD_ADDRESS: u64 = 0x9100_0000;
pub(crate) const SCU_BASE: u64 = 0x6000_0000;
pub(crate) const SCU_STRIDE: u64 = 0x4_0000;

struct Uart {
    rx: VecDeque<u8>,
    tx: Vec<u8>,
    log: Option<BufWriter<File>>,
    cycle_trace: Option<CycleTraceCollector>,
}

pub(crate) struct Platform {
    pub(crate) interconnect: Option<super::interconnect::port::Port>,
    pub(crate) memory: Arc<super::memory::Ddr>,
    pub(crate) pages: Arc<Mutex<super::memory::Pages>>,
    pub(crate) clint: Arc<Clint>,
    pub(crate) microphone: bebop_microphone::Microphone,
    pub(crate) speaker: bebop_speaker::Speaker,
    pub(crate) keyboard: bebop_keyboard::Keyboard,
    pub(crate) vga: bebop_vga::Vga,
    pub(crate) rtc: bebop_rtc::Rtc,
    clock: u64,
    plic: Plic,
    uarts: Vec<Uart>,
    pub(crate) capture_uart: bool,
    pub(crate) console_uart: bool,
    pub(crate) exit_codes: Arc<Vec<AtomicI64>>,
    pub(crate) exit_code: Arc<AtomicI64>,
}

impl Platform {
    pub(crate) fn new(size: usize, harts: usize) -> Self {
        Self {
            memory: Arc::new(super::memory::Ddr::new(size)),
            pages: Arc::new(Mutex::new(super::memory::Pages::new(size))),
            interconnect: None,
            clint: Arc::new(Clint::new(harts)),
            microphone: bebop_microphone::Microphone::default(),
            speaker: bebop_speaker::Speaker::default(),
            keyboard: bebop_keyboard::Keyboard::default(),
            vga: bebop_vga::Vga::default(),
            rtc: bebop_rtc::Rtc::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("RTC epoch")
                    .as_nanos()
                    .try_into()
                    .expect("RTC range"),
            ),
            clock: 0,
            plic: Plic::default(),
            uarts: (0..harts)
                .map(|_| Uart {
                    rx: VecDeque::new(),
                    tx: Vec::new(),
                    log: None,
                    cycle_trace: None,
                })
                .collect(),
            capture_uart: false,
            console_uart: true,
            exit_codes: Arc::new((0..harts).map(|_| AtomicI64::new(i64::MIN)).collect()),
            exit_code: Arc::new(AtomicI64::new(i64::MIN)),
        }
    }

    pub(crate) fn sync_clock(&mut self) {
        let next = self.clint.cycles();
        let elapsed = next - self.clock;
        let ns = elapsed.checked_mul(1_000_000_000 / bebop_clint::SOC_CLOCK_HZ).expect("simulation time overflow");
        self.microphone.tick(ns);
        self.speaker.tick(ns);
        self.rtc.tick(ns);
        self.clock = next;
    }

    pub(crate) fn push_uart(&mut self, hart: usize, byte: u8) {
        self.uarts[hart].rx.push_back(byte);
    }

    pub(crate) fn take_uart(&mut self) -> Vec<(u32, Vec<u8>)> {
        self.uarts
            .iter_mut()
            .enumerate()
            .filter(|(_, uart)| !uart.tx.is_empty())
            .map(|(hart, uart)| (hart as u32, std::mem::take(&mut uart.tx)))
            .collect()
    }

    pub(crate) fn uart_log(&mut self, hart: usize, file: File, cycle_trace: CycleTraceCollector) {
        self.uarts[hart].log = Some(BufWriter::new(file));
        self.uarts[hart].cycle_trace = Some(cycle_trace);
    }

    pub(crate) fn flush_uart(&mut self, hart: usize) -> std::io::Result<()> {
        if let Some(log) = self.uarts[hart].log.as_mut() {
            log.flush()?;
        }
        if let Some(trace) = self.uarts[hart].cycle_trace.take() {
            trace.finish().map_err(std::io::Error::other)?;
        }
        Ok(())
    }
}

impl Bus for Platform {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        self.sync_clock();
        use super::interconnect::port::{BASE, SIZE};
        if (BASE..BASE + SIZE).contains(&address) {
            return self.interconnect.as_ref().ok_or(BusError)?.read(address - BASE, width);
        }
        if address >= DRAM_BASE {
            return self.memory.read(address, width);
        }
        if (bebop_clint::BASE..bebop_clint::BASE + bebop_clint::SIZE).contains(&address) {
            return self
                .clint
                .load(address - bebop_clint::BASE, width as usize)
                .ok_or(BusError);
        }
        if (bebop_plic::BASE..bebop_plic::BASE + bebop_plic::SIZE).contains(&address) {
            return self
                .plic
                .load(address - bebop_plic::BASE, width as usize)
                .ok_or(BusError);
        }
        if (bebop_microphone::BASE..bebop_microphone::BASE + bebop_microphone::SIZE).contains(&address) {
            return self
                .microphone
                .load(address - bebop_microphone::BASE, width as usize)
                .ok_or(BusError);
        }
        if (bebop_speaker::BASE..bebop_speaker::BASE + bebop_speaker::SIZE).contains(&address) {
            return self
                .speaker
                .load(address - bebop_speaker::BASE, width as usize)
                .ok_or(BusError);
        }
        if (bebop_keyboard::BASE..bebop_keyboard::BASE + bebop_keyboard::SIZE).contains(&address) {
            return self
                .keyboard
                .load(address - bebop_keyboard::BASE, width as usize)
                .ok_or(BusError);
        }
        if (bebop_vga::BASE..bebop_vga::BASE + bebop_vga::SIZE).contains(&address) {
            return self.vga.load(address - bebop_vga::BASE, width as usize).ok_or(BusError);
        }
        if (bebop_rtc::BASE..bebop_rtc::BASE + bebop_rtc::SIZE).contains(&address) {
            return self.rtc.load(address - bebop_rtc::BASE, width as usize).ok_or(BusError);
        }
        if (SCU_BASE..SCU_BASE + self.uarts.len() as u64 * SCU_STRIDE).contains(&address) {
            let uart = &mut self.uarts[((address - SCU_BASE) / SCU_STRIDE) as usize];
            return match ((address - SCU_BASE) % SCU_STRIDE, width) {
                (0x20004, Width::Byte) => Ok(uart.rx.pop_front().map_or(0, u64::from)),
                (0x20005, Width::Byte) => Ok(u64::from(!uart.rx.is_empty())),
                _ => Err(BusError),
            };
        }
        Err(BusError)
    }

    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        self.sync_clock();
        use super::interconnect::port::{BASE, SIZE};
        if (BASE..BASE + SIZE).contains(&address) {
            return self
                .interconnect
                .as_mut()
                .ok_or(BusError)?
                .write(address - BASE, width, value);
        }
        if address >= DRAM_BASE {
            return self.memory.write(address, width, value);
        }
        if (bebop_clint::BASE..bebop_clint::BASE + bebop_clint::SIZE).contains(&address) {
            return self
                .clint
                .store(address - bebop_clint::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (bebop_plic::BASE..bebop_plic::BASE + bebop_plic::SIZE).contains(&address) {
            return self
                .plic
                .store(address - bebop_plic::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (bebop_microphone::BASE..bebop_microphone::BASE + bebop_microphone::SIZE).contains(&address) {
            return self
                .microphone
                .store(address - bebop_microphone::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (bebop_speaker::BASE..bebop_speaker::BASE + bebop_speaker::SIZE).contains(&address) {
            return self
                .speaker
                .store(address - bebop_speaker::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (bebop_keyboard::BASE..bebop_keyboard::BASE + bebop_keyboard::SIZE).contains(&address) {
            return Err(BusError);
        }
        if (bebop_vga::BASE..bebop_vga::BASE + bebop_vga::SIZE).contains(&address) {
            return self
                .vga
                .store(address - bebop_vga::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (bebop_rtc::BASE..bebop_rtc::BASE + bebop_rtc::SIZE).contains(&address) {
            return self
                .rtc
                .store(address - bebop_rtc::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (SCU_BASE..SCU_BASE + self.uarts.len() as u64 * SCU_STRIDE).contains(&address) {
            let hart = ((address - SCU_BASE) / SCU_STRIDE) as usize;
            return match ((address - SCU_BASE) % SCU_STRIDE, width) {
                (0, Width::Word | Width::Double) => {
                    self.exit_codes[hart].store(value as i32 as i64, Ordering::Release);
                    let _ = self.exit_code.compare_exchange(
                        i64::MIN, value as i32 as i64, Ordering::AcqRel, Ordering::Acquire,
                    );
                    Ok(())
                }
                (0x20000, Width::Byte | Width::Word) => {
                    let byte = [value as u8];
                    if self.capture_uart {
                        self.uarts[hart].tx.push(byte[0]);
                    }
                    if let Some(log) = self.uarts[hart].log.as_mut() {
                        log.write_all(&byte).expect("write BEMU UART log");
                    }
                    if let Some(trace) = self.uarts[hart].cycle_trace.as_mut() {
                        trace
                            .push_uart_byte(hart as u32, byte[0])
                            .expect("collect BEMU cycle trace");
                    }
                    if self.console_uart {
                        let mut output = std::io::stdout().lock();
                        output.write_all(&byte).expect("write BEMU UART");
                        output.flush().expect("flush BEMU UART");
                    }
                    Ok(())
                }
                _ => Err(BusError),
            };
        }
        Err(BusError)
    }

    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        self.memory.compare_exchange(address, width, expected, value)
    }
}

impl HartBus for Platform {
    fn load_reserved(&mut self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        self.memory.load_reserved(hart, address, width)
    }

    fn store_conditional(&mut self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        self.memory.store_conditional(hart, address, width, value)
    }
}

pub(crate) struct Port<'a> {
    pub(crate) devices: &'a Mutex<Platform>,
    pub(crate) memory: &'a super::memory::Ddr,
}

impl Bus for Port<'_> {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        if address >= DRAM_BASE {
            return self.memory.read(address, width);
        }
        self.devices
            .lock()
            .expect("BEMU platform poisoned")
            .read(address, width)
    }

    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        if address >= DRAM_BASE {
            return self.memory.write(address, width, value);
        }
        self.devices
            .lock()
            .expect("BEMU platform poisoned")
            .write(address, width, value)
    }

    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        self.memory.compare_exchange(address, width, expected, value)
    }
}

impl HartBus for Port<'_> {
    fn load_reserved(&mut self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        self.memory.load_reserved(hart, address, width)
    }

    fn store_conditional(&mut self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        self.memory.store_conditional(hart, address, width, value)
    }
}
