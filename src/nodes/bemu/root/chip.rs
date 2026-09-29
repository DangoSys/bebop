use super::platform::Platform;
use std::sync::{Arc, Mutex};

pub struct Chip {
    pub(crate) platform: Arc<Mutex<Platform>>,
}

impl Chip {
    pub fn push_live_microphone(&self, samples: Vec<i16>) -> Result<(), &'static str> {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .microphone
            .push_live(samples)
    }

    pub fn clock(&self) -> u64 {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .rtc
            .load(0, 8)
            .unwrap()
    }

    pub fn set_clock(&self, ns: u64) {
        assert!(self
            .platform
            .lock()
            .expect("BEMU platform poisoned")
            .rtc
            .store(0, 8, ns));
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
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .microphone
            .push(samples);
    }

    pub fn push_key(&self, code: u16, pressed: bool) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .keyboard
            .push(code, pressed);
    }

    pub fn take_audio(&self) -> Vec<i16> {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .speaker
            .take_samples()
    }

    pub fn take_frame(&self) -> Option<Vec<u8>> {
        self.platform.lock().expect("BEMU platform poisoned").vga.take_frame()
    }

    pub fn new(memory_size: usize, hart_count: usize) -> Self {
        Self {
            platform: Arc::new(Mutex::new(Platform::new(memory_size, hart_count))),
        }
    }

    pub fn push_uart(&self, hart: usize, byte: u8) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .push_uart(hart, byte);
    }
}
