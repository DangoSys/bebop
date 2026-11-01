use super::platform::Platform;
use crate::core::CoreState;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};
pub trait Service: Send + Sync {
    fn request(&self, core: usize, operation: u32, address: u64, data: u64) -> u64;
}

pub struct DeviceModel {
    pub(crate) timing: Arc<crate::sim::timing::DeviceTime>,
    pub(crate) instances: Mutex<Vec<Arc<CoreState>>>,
    pub(crate) cores: Arc<Mutex<BTreeMap<usize, Weak<CoreState>>>>,
    pub(crate) service: Mutex<Option<Arc<dyn Service>>>,
    pub(crate) clint: Arc<crate::device::clint::Clint>,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) memory: Arc<super::memory::Ddr>,
}

impl DeviceModel {
    pub fn set_service(&self, service: Arc<dyn Service>) {
        assert!(self.instances.lock().expect("Device instances poisoned").is_empty());
        assert!(self
            .service
            .lock()
            .expect("Device service poisoned")
            .replace(service)
            .is_none());
    }

    pub fn coordinate_harts(&self, harts: Vec<usize>) {
        self.clint.coordinate(harts);
        let clocks = self
            .instances
            .lock()
            .unwrap()
            .iter()
            .flat_map(|core| core.tasks.contexts.iter().map(|context| Arc::clone(&context.progress)))
            .collect();
        self.timing
            .threads
            .set(clocks)
            .expect("time participants configured twice");
        let caches = self
            .instances
            .lock()
            .unwrap()
            .iter()
            .map(|core| Arc::clone(&core.cache))
            .collect();
        assert!(self.timing.cpus.set(caches).is_ok(), "CPU caches configured twice");
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

    pub fn new(memory_size: usize, hart_count: usize, log_dir: &std::path::Path) -> Self {
        let platform = Platform::new(memory_size, hart_count, log_dir);
        Self {
            timing: Arc::new(crate::sim::timing::DeviceTime::new(Arc::clone(&platform.clint))),
            instances: Mutex::new(Vec::new()),
            cores: Arc::new(Mutex::new(BTreeMap::new())),
            service: Mutex::new(None),
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
