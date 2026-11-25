use super::platform::Platform;
use generic::hlink::{HLink, Peripherals};
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests {
    use super::*;
    use generic::configs::design::{DdrHardware, Design, DeviceConfig, TraceHardware};
    use prost::Message;

    #[test]
    fn ddr_image_then_elf_overlay_and_elf_only() {
        let base = generic::ddr::DRAM_BASE;
        let bytes = 3 << 20;
        let device = DeviceConfig {
            ddr: Some(DdrHardware {
                base,
                bytes: bytes as u64,
                beat_rows: 4,
                ..Default::default()
            }),
            trace: Some(TraceHardware::default()),
            ..Default::default()
        };
        generic::configs::load(
            &Design {
                device: Some(device),
                ..Default::default()
            }
            .encode_to_vec(),
        );
        let dir = std::env::temp_dir().join(format!("bebop-ddr-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut image = vec![0; (2 << 20) + 17];
        image[0x200..0x204].copy_from_slice(&[1, 3, 5, 7]);
        image[0x1000..0x1008].fill(0xa5);
        image[2 << 20] = 0x91;
        std::fs::write(dir.join("weights.bin"), image).unwrap();
        // One standalone ELF load segment, with four file bytes and four BSS bytes.
        let mut elf = vec![0; 132];
        elf[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
        elf[16..18].copy_from_slice(&2u16.to_le_bytes());
        elf[18..20].copy_from_slice(&243u16.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[24..32].copy_from_slice(&(base + 0x1000).to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[52..54].copy_from_slice(&64u16.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());
        elf[64..68].copy_from_slice(&1u32.to_le_bytes());
        elf[68..72].copy_from_slice(&7u32.to_le_bytes());
        for (offset, value) in [
            (72, 128),
            (80, base + 0x1000),
            (88, base + 0x1000),
            (96, 4),
            (104, 8),
            (112, 1),
        ] {
            elf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }
        elf[128..].copy_from_slice(&[2, 4, 6, 8]);
        let path = dir.join("entry.elf");
        std::fs::write(&path, elf).unwrap();
        for with_ddr in [false, true] {
            let model = DeviceModel::new(bytes, 1, &dir);
            if with_ddr {
                model.load_ddr_image(&dir.join("weights.bin")).unwrap();
            }
            bebop_elf::load_elf(path.to_str().unwrap(), model.link.memory.as_ref(), base, 0).unwrap();
            let mut payload = [0; 8];
            model.link.memory.read_buffer(base + 0x1000, &mut payload).unwrap();
            assert_eq!(payload, [2, 4, 6, 8, 0, 0, 0, 0]);
            model.link.memory.read_buffer(base + 0x200, &mut payload[..4]).unwrap();
            assert_eq!(&payload[..4], if with_ddr { &[1, 3, 5, 7] } else { &[0; 4] });
            model
                .link
                .memory
                .read_buffer(base + (2 << 20), &mut payload[..1])
                .unwrap();
            assert_eq!(payload[0], if with_ddr { 0x91 } else { 0 });
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// A device: its peripherals, and the link it offers each of its harts.
pub struct DeviceModel {
    pub link: HLink,
    pub platform: Arc<Mutex<Platform>>,
}

impl DeviceModel {
    pub fn load_ddr_image(&self, image: &std::path::Path) -> Result<(), String> {
        let mut file = std::fs::File::open(image).map_err(|error| format!("DDR image {}: {error}", image.display()))?;
        let mut chunk = vec![0u8; 1 << 20];
        let mut address = generic::configs::ddr().0;
        loop {
            let count = std::io::Read::read(&mut file, &mut chunk).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            // The image is sparse: do not allocate the pages that are zero.
            if chunk[..count].iter().any(|byte| *byte != 0) {
                self.link
                    .memory
                    .write_buffer(address, &chunk[..count])
                    .map_err(|_| "DDR image outside the DDR".to_string())?;
            }
            address += count as u64;
        }
        Ok(())
    }

    pub fn coordinate_harts(&self, harts: Vec<usize>, concurrent: bool) {
        self.link.clint.coordinate(harts, concurrent);
        let attached = self.link.harts.lock().unwrap();
        let clocks = attached
            .iter()
            .flat_map(|hart| hart.threads.iter().map(Arc::clone))
            .collect();
        assert!(
            self.link.timing.threads.set(clocks).is_ok(),
            "time participants configured twice"
        );
        let caches = attached.iter().map(|hart| Arc::clone(&hart.cache)).collect();
        assert!(self.link.timing.cpus.set(caches).is_ok(), "CPU caches configured twice");
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
        let timing = Arc::new(generic::timing::DeviceTime::new(
            Arc::clone(&platform.clint),
            generic::configs::ddr_beat_rows(),
        ));
        let clint = Arc::clone(&platform.clint);
        let memory = Arc::clone(&platform.memory);
        let pages = Arc::clone(&platform.pages);
        let exit_code = Arc::clone(&platform.exit_code);
        let platform = Arc::new(Mutex::new(platform));
        let peripherals: Arc<Mutex<dyn Peripherals>> = platform.clone();
        Self {
            link: HLink {
                capacity: hart_count,
                timing,
                clint,
                memory,
                pages,
                peripherals,
                exit_code,
                service: Mutex::new(None),
                harts: Mutex::new(Vec::new()),
            },
            platform,
        }
    }

    pub fn push_uart(&self, hart: usize, byte: u8) {
        self.platform
            .lock()
            .expect("BEMU platform poisoned")
            .push_uart(hart, byte);
    }
}
