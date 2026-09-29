use bebop_clint::Clint;
use bebop_plic::Plic;
use rvsim::bus::{Bus, BusError, HartBus, Width};
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufWriter, Write},
    sync::{Arc, Mutex},
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
}

pub(crate) struct Platform {
    pub(crate) interconnect: Option<super::interconnect::port::Port>,
    pub(crate) memory: Vec<u8>,
    pub(crate) pages: Arc<Mutex<super::memory::Pages>>,
    pub(crate) clint: Clint,
    pub(crate) microphone: bebop_microphone::Microphone,
    pub(crate) speaker: bebop_speaker::Speaker,
    pub(crate) keyboard: bebop_keyboard::Keyboard,
    pub(crate) vga: bebop_vga::Vga,
    pub(crate) rtc: bebop_rtc::Rtc,
    cycles: Vec<u64>,
    clock: u64,
    plic: Plic,
    uarts: Vec<Uart>,
    pub(crate) capture_uart: bool,
    pub(crate) console_uart: bool,
    pub(crate) exit_codes: Vec<Option<i32>>,
    pub(crate) reservations: HashMap<u64, (u64, Width)>,
}

impl Platform {
    pub(crate) fn new(size: usize, harts: usize) -> Self {
        Self {
            memory: vec![0; size],
            pages: Arc::new(Mutex::new(super::memory::Pages::new(size))),
            interconnect: None,
            clint: Clint::new(harts),
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
            cycles: vec![0; harts],
            clock: 0,
            plic: Plic::default(),
            uarts: (0..harts)
                .map(|_| Uart {
                    rx: VecDeque::new(),
                    tx: Vec::new(),
                    log: None,
                })
                .collect(),
            capture_uart: false,
            console_uart: true,
            exit_codes: vec![None; harts],
            reservations: HashMap::new(),
        }
    }

    pub(crate) fn inputs(&mut self, hart: usize, cycles: u64) -> rvsim::hart::Inputs {
        self.cycles[hart] += cycles;
        let next = self.clock.max(self.cycles[hart]);
        let elapsed = next - self.clock;
        self.clint.tick(elapsed);
        let ns = elapsed.checked_mul(100).expect("simulation time overflow"); // 10 MHz timebase.
        self.microphone.tick(ns);
        self.speaker.tick(ns);
        self.rtc.tick(ns);
        self.clock = next;
        rvsim::hart::Inputs {
            cycles,
            time: self.clint.time(),
            interrupts: self.clint.pending(hart),
        }
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

    pub(crate) fn uart_log(&mut self, hart: usize, file: File) {
        self.uarts[hart].log = Some(BufWriter::new(file));
    }

    pub(crate) fn flush_uart(&mut self, hart: usize) -> std::io::Result<()> {
        if let Some(log) = self.uarts[hart].log.as_mut() {
            log.flush()?;
        }
        Ok(())
    }
}

impl Bus for Platform {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        use super::interconnect::port::{BASE, SIZE};
        if (BASE..BASE + SIZE).contains(&address) {
            return self.interconnect.as_ref().ok_or(BusError)?.read(address - BASE, width);
        }
        if address >= DRAM_BASE {
            let offset = (address - DRAM_BASE) as usize;
            let source = self.memory.get(offset..offset + width as usize).ok_or(BusError)?;
            let mut value = [0; 8];
            value[..width as usize].copy_from_slice(source);
            return Ok(u64::from_le_bytes(value));
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
        use super::interconnect::port::{BASE, SIZE};
        if (BASE..BASE + SIZE).contains(&address) {
            return self.interconnect.as_mut().ok_or(BusError)?.write(address - BASE, width, value);
        }
        if address >= DRAM_BASE {
            let offset = (address - DRAM_BASE) as usize;
            self.memory
                .get_mut(offset..offset + width as usize)
                .ok_or(BusError)?
                .copy_from_slice(&value.to_le_bytes()[..width as usize]);
            self.reservations
                .retain(|_, (start, size)| *start + *size as u64 <= address || address + width as u64 <= *start);
            return Ok(());
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
                    self.exit_codes[hart] = Some(value as i32);
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

    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        if address < DRAM_BASE {
            return Err(BusError);
        }
        let old = self.read(address, width)?;
        if old == expected {
            self.write(address, width, value)?;
        }
        Ok(old)
    }
}

impl HartBus for Platform {
    fn load_reserved(&mut self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        if address < DRAM_BASE {
            return Err(BusError);
        }
        let value = self.read(address, width)?;
        self.reservations.insert(hart, (address, width));
        Ok(value)
    }

    fn store_conditional(&mut self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        if address < DRAM_BASE {
            return Err(BusError);
        }
        self.read(address, width)?;
        let success = self.reservations.remove(&hart) == Some((address, width));
        if success {
            self.write(address, width, value)?;
        }
        Ok(success)
    }
}

pub(crate) struct Port<'a>(pub(crate) &'a std::sync::Mutex<Platform>);

impl Bus for Port<'_> {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        self.0.lock().expect("BEMU platform poisoned").read(address, width)
    }

    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        self.0
            .lock()
            .expect("BEMU platform poisoned")
            .write(address, width, value)
    }

    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        self.0
            .lock()
            .expect("BEMU platform poisoned")
            .compare_exchange(address, width, expected, value)
    }
}

impl HartBus for Port<'_> {
    fn load_reserved(&mut self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        self.0
            .lock()
            .expect("BEMU platform poisoned")
            .load_reserved(hart, address, width)
    }

    fn store_conditional(&mut self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        self.0
            .lock()
            .expect("BEMU platform poisoned")
            .store_conditional(hart, address, width, value)
    }
}
