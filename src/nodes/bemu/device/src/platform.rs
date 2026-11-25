use crate::plic::Plic;
use bebop_cycle_trace::CycleTrace;
use generic::clint::Clint;
use generic::ddr::DRAM_BASE;
use generic::riscv::bus::{Bus, BusError, HartBus, Width};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
};

pub(crate) const SCU_BASE: u64 = 0x6000_0000;
pub(crate) const SCU_STRIDE: u64 = 0x4_0000;

struct Uart {
    rx: VecDeque<u8>,
    tx: Vec<u8>,
    log: Option<BufWriter<File>>,
}

pub struct Platform {
    pub(crate) memory: Arc<generic::ddr::Ddr>,
    pub(crate) pages: Arc<Mutex<generic::ddr::Pages>>,
    pub(crate) clint: Arc<Clint>,
    pub(crate) microphone: bebop_microphone::Microphone,
    pub(crate) speaker: bebop_speaker::Speaker,
    pub(crate) keyboard: bebop_keyboard::Keyboard,
    pub(crate) vga: bebop_vga::Vga,
    pub(crate) rtc: bebop_rtc::Rtc,
    clock: u64,
    /// An access since the bus was last asked left memory.
    peripheral: bool,
    plic: Plic,
    uarts: Vec<Uart>,
    pub trace: CycleTrace,
    trace_base: u64,
    trace_bytes: u64,
    trace_stride: u64,
    pub(crate) capture_uart: bool,
    pub(crate) console_uart: bool,
    pub(crate) exit_codes: Arc<Vec<AtomicI64>>,
    pub(crate) exit_code: Arc<AtomicI64>,
    pub output_capture: Option<bebop_elf::OutputCapture>,
}

impl Platform {
    pub(crate) fn new(size: usize, harts: usize, log_dir: &Path) -> Self {
        Self {
            memory: Arc::new(generic::ddr::Ddr::new(size)),
            pages: Arc::new(Mutex::new(generic::ddr::Pages::new(size))),
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
            peripheral: false,
            plic: Plic::default(),
            uarts: (0..harts)
                .map(|_| Uart {
                    rx: VecDeque::new(),
                    tx: Vec::new(),
                    log: None,
                })
                .collect(),
            capture_uart: false,
            trace: CycleTrace::new(log_dir),
            trace_base: generic::configs::design::trace().base,
            trace_bytes: generic::configs::design::trace().bytes,
            trace_stride: u64::from(generic::configs::design::trace().stride),
            console_uart: true,
            exit_codes: Arc::new((0..harts).map(|_| AtomicI64::new(i64::MIN)).collect()),
            exit_code: Arc::new(AtomicI64::new(i64::MIN)),
            output_capture: None,
        }
    }

    pub(crate) fn sync_clock(&mut self) {
        let next = self.clint.cycles();
        let elapsed = next - self.clock;
        let ns = elapsed
            .checked_mul(1_000_000_000 / generic::clint::SOC_CLOCK_HZ)
            .expect("simulation time overflow");
        self.microphone.tick(ns);
        self.speaker.tick(ns);
        self.rtc.tick(ns);
        self.clock = next;
    }

    pub fn push_uart(&mut self, hart: usize, byte: u8) {
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
}

impl generic::hlink::Peripherals for Platform {
    fn memory(&self) -> &generic::ddr::Ddr {
        &self.memory
    }

    fn uart_log(&mut self, hart: usize, file: File) {
        self.uarts[hart].log = Some(BufWriter::new(file));
    }

    fn flush_uart(&mut self, hart: usize) -> std::io::Result<()> {
        if let Some(log) = self.uarts[hart].log.as_mut() {
            log.flush()?;
        }
        Ok(())
    }

    fn capture_output(&mut self, capture: Option<bebop_elf::OutputCapture>) {
        self.output_capture = capture;
    }
}

impl Bus for Platform {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        self.sync_clock();
        if address >= DRAM_BASE {
            return self.memory.read(address, width);
        }
        self.peripheral = true;
        if (generic::clint::BASE..generic::clint::BASE + generic::clint::SIZE).contains(&address) {
            return self
                .clint
                .load(address - generic::clint::BASE, width as usize)
                .ok_or(BusError);
        }
        if (crate::plic::BASE..crate::plic::BASE + crate::plic::SIZE).contains(&address) {
            return self
                .plic
                .load(address - crate::plic::BASE, width as usize)
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
        if address >= DRAM_BASE {
            return self.memory.write(address, width, value);
        }
        self.peripheral = true;
        if (self.trace_base..self.trace_base + self.trace_bytes).contains(&address) {
            let offset = address - self.trace_base;
            let channel = (offset / self.trace_stride) as usize;
            let register = offset % self.trace_stride;
            if width != Width::Double || register % 8 != 0 || register >= 96 {
                return Err(BusError);
            }
            self.trace
                .write(channel as u32, (register / 8) as usize, value)
                .expect("record cycle trace event");
            return Ok(());
        }
        if (generic::clint::BASE..generic::clint::BASE + generic::clint::SIZE).contains(&address) {
            return self
                .clint
                .store(address - generic::clint::BASE, width as usize, value)
                .then_some(())
                .ok_or(BusError);
        }
        if (crate::plic::BASE..crate::plic::BASE + crate::plic::SIZE).contains(&address) {
            return self
                .plic
                .store(address - crate::plic::BASE, width as usize, value)
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
                    if let Some(output) = &mut self.output_capture {
                        if output.push(hart, value as u32).map_err(|_| BusError)? {
                            return Ok(());
                        }
                    }
                    self.exit_codes[hart].store(value as i32 as i64, Ordering::Release);
                    let _ = self.exit_code.compare_exchange(
                        i64::MIN,
                        value as i32 as i64,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                    self.clint.halt();
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

    fn peripheral(&mut self) -> bool {
        std::mem::take(&mut self.peripheral)
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
