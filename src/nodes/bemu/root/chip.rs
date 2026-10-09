use super::platform::Platform;
use std::{collections::BTreeMap, sync::{Arc, Mutex, Weak}};

pub struct Chip {
    pub(crate) tiles: Arc<Mutex<BTreeMap<usize, Weak<super::tile::Tile>>>>,
    pub(crate) clint: Arc<bebop_clint::Clint>,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) memory: Arc<super::memory::Ddr>,
}

impl Chip {
    pub fn coordinate_harts(&self, harts: Vec<usize>) {
        self.clint.coordinate(harts);
    }

    pub fn push_live_microphone(&self, samples: Vec<i16>) -> Result<(), &'static str> {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.microphone.push_live(samples)
    }

    pub fn clock(&self) -> u64 {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.rtc.load(0, 8).unwrap()
    }

    pub fn set_clock(&self, ns: u64) {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        assert!(platform.rtc.store(0, 8, ns));
    }

    pub fn hart_count(&self) -> usize {
        self.platform.lock().expect("BEMU platform poisoned").exit_codes.len()
    }

    pub fn capture_uart(&self) {
        self.platform.lock().expect("BEMU platform poisoned").capture_uart = true;
    }

    pub fn take_uart(&self) -> Vec<(u32, Vec<u8>)> {
        self.platform.lock().expect("BEMU platform poisoned").take_uart()
    }
    pub fn push_microphone(&self, samples: impl IntoIterator<Item = i16>) {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.microphone.push(samples);
    }

    pub fn push_key(&self, code: u16, pressed: bool) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .keyboard
            .push(code, pressed);
    }

    pub fn take_audio(&self) -> Vec<i16> {
        let mut platform = self.platform.lock().expect("BEMU platform poisoned");
        platform.sync_clock();
        platform.speaker.take_samples()
    }

    pub fn take_frame(&self) -> Option<Vec<u8>> {
        self.platform.lock().expect("BEMU platform poisoned").vga.take_frame()
    }

    pub fn new(memory_size: usize, hart_count: usize) -> Self {
        let platform = Platform::new(memory_size, hart_count);
        Self {
            tiles: Arc::new(Mutex::new(BTreeMap::new())),
            clint: Arc::clone(&platform.clint),
            memory: Arc::clone(&platform.memory),
            platform: Arc::new(Mutex::new(platform)),
        }
    }

    pub fn push_uart(&self, hart: usize, byte: u8) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .push_uart(hart, byte);
    }
}
