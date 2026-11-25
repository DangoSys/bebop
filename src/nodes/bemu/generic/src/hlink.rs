//! HLink: what a device and each of its harts see of each other.

use crate::{
    clint::Clint,
    ddr::{Ddr, Pages, DRAM_BASE},
    riscv::bus::{Bus, BusError, HartBus, Width},
    timing::{CpuCache, DeviceTime, ServicePhases},
};
use std::{
    fs::File,
    sync::{
        atomic::{AtomicI64, AtomicU64},
        Arc, Mutex,
    },
};

pub const PLIC_BASE: u64 = 0x0c00_0000;
pub const PLIC_SIZE: u64 = 0x0400_0000;

/// A device service on the CBus; ordinary memory preparation belongs to the hart.
pub trait Service: Send + Sync {
    fn request(&self, hart: usize, operation: u32, address: u64, data: u64) -> (u64, u64);
    /// Exchange a hart-owned memory payload; the device never accesses DDR.
    fn exchange(&self, hart: usize, to_device: bool, bytes: &mut [u8]) -> ServicePhases;
}

/// The peripherals of the device as a hart reaches them: the uncached bus, its own UART log
/// and the output capture of the program it loads. Their lock is the device DDR lock.
pub trait Peripherals: HartBus + Send {
    fn memory(&self) -> &Ddr;
    fn uart_log(&mut self, hart: usize, file: File);
    fn flush_uart(&mut self, hart: usize) -> std::io::Result<()>;
    fn capture_output(&mut self, capture: Option<bebop_elf::OutputCapture>);
}

/// A hart's port to the device memory map: the DDR directly, the peripherals behind their lock.
pub struct Port<'a> {
    pub cache: Option<&'a CpuCache>,
    pub devices: &'a Mutex<dyn Peripherals>,
    pub memory: &'a Ddr,
    /// An access since the port was last asked went to the peripherals.
    pub peripheral: bool,
}

impl Bus for Port<'_> {
    #[inline]
    fn fetch(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        if address >= DRAM_BASE {
            return self.memory.read(address, width);
        }
        self.peripheral = true;
        self.devices
            .lock()
            .expect("BEMU platform poisoned")
            .read(address, width)
    }

    #[inline]
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        let value = self.fetch(address, width)?;
        if address >= DRAM_BASE {
            if let Some(cache) = self.cache {
                cache.access(address, false);
            }
        }
        Ok(value)
    }

    #[inline]
    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        if address >= DRAM_BASE {
            self.memory.write(address, width, value)?;
            if let Some(cache) = self.cache {
                cache.access(address, true);
            }
            return Ok(());
        }
        self.peripheral = true;
        self.devices
            .lock()
            .expect("BEMU platform poisoned")
            .write(address, width, value)
    }

    fn peripheral(&mut self) -> bool {
        std::mem::take(&mut self.peripheral)
    }

    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        let old = self.memory.compare_exchange(address, width, expected, value)?;
        if let Some(cache) = self.cache {
            cache.access(address, old == expected);
        }
        Ok(old)
    }
}

impl HartBus for Port<'_> {
    fn load_reserved(&mut self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        let value = self.memory.load_reserved(hart, address, width)?;
        if let Some(cache) = self.cache {
            cache.access(address, false);
        }
        Ok(value)
    }

    fn store_conditional(&mut self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        let success = self.memory.store_conditional(hart, address, width, value)?;
        if success {
            if let Some(cache) = self.cache {
                cache.access(address, true);
            }
        }
        Ok(success)
    }
}

/// What a hart reports to the device it is attached to: its CPU cache and the clocks of its
/// hardware threads, which the device memory timing consults.
pub struct Attachment {
    pub hart_id: usize,
    pub cache: Arc<CpuCache>,
    pub threads: Vec<Arc<AtomicU64>>,
}

/// The device side of every hart's link: the time base, the DDR and its timing, the
/// peripherals and the device service. A hart is built from this and attaches itself to it.
pub struct HLink {
    /// The harts the device has room for.
    pub capacity: usize,
    pub timing: Arc<DeviceTime>,
    pub clint: Arc<Clint>,
    pub memory: Arc<Ddr>,
    pub pages: Arc<Mutex<Pages>>,
    pub peripherals: Arc<Mutex<dyn Peripherals>>,
    pub exit_code: Arc<AtomicI64>,
    pub service: Mutex<Option<Arc<dyn Service>>>,
    pub harts: Mutex<Vec<Attachment>>,
}

impl HLink {
    pub fn set_service(&self, service: Arc<dyn Service>) {
        assert!(self.harts.lock().expect("Device harts poisoned").is_empty());
        assert!(self
            .service
            .lock()
            .expect("Device service poisoned")
            .replace(service)
            .is_none());
    }

    pub fn attach(&self, hart: Attachment) {
        let mut harts = self.harts.lock().expect("Device harts poisoned");
        assert!(
            harts.iter().all(|other| other.hart_id != hart.hart_id),
            "duplicate live design hart"
        );
        harts.push(hart);
    }
}
