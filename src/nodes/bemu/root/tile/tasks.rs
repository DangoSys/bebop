pub(crate) use super::workers::Workers;
use super::workers::{Barrier, Execution};
use crate::config;
use rvsim::{bus::BusError, hart::Hart};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};

pub(super) struct Memory {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
}
impl Memory {
    pub(super) fn new(base: u64, bytes: usize) -> Self {
        Self {
            base,
            bytes: vec![0xa5; bytes],
        }
    }
    pub(super) fn offset(&self, address: u64, count: usize) -> Option<usize> {
        let offset = address.checked_sub(self.base)?;
        (offset.checked_add(count as u64)? <= self.bytes.len() as u64).then_some(offset as usize)
    }
    pub(super) fn read(&self, address: u64, count: usize) -> Result<u64, BusError> {
        let offset = self.offset(address, count).ok_or(BusError)?;
        let mut data = [0; 8];
        data[..count].copy_from_slice(&self.bytes[offset..offset + count]);
        Ok(u64::from_le_bytes(data))
    }
    pub(super) fn write(&mut self, address: u64, count: usize, value: u64) -> Result<(), BusError> {
        let offset = self.offset(address, count).ok_or(BusError)?;
        self.bytes[offset..offset + count].copy_from_slice(&value.to_le_bytes()[..count]);
        Ok(())
    }
}
pub(super) struct Control {
    pub(super) dma: Option<(rvsim::csr::Csrs, rvsim::Privilege)>,
    pub(super) descriptor: [u64; 6],
    fields: u8,
    pub(super) active: bool,
    pub(super) done: bool,
    pub(super) value: u64,
}
pub(super) struct Context {
    pub(super) core: usize,
    signature: u64,
    pub(super) params: config::AntConfig,
    pub(super) execution: Mutex<Execution>,
    pub(super) control: Mutex<Control>,
    pub(super) ready: Condvar,
    pub(super) cancelled: AtomicBool,
}
pub(crate) struct Tasks {
    pub(super) contexts: Vec<Arc<Context>>,
    pub(super) tss: Option<Mutex<Memory>>,
    pub(super) tss_range: Option<(u64, usize)>,
    group: Mutex<bool>,
    pub(super) barrier: Mutex<Barrier>,
    pub(super) barrier_ready: Condvar,
    pub(super) started: AtomicBool,
    pub(super) stopped: AtomicBool,
    pub(super) failure: Mutex<Option<String>>,
}
impl Tasks {
    pub fn new(cores: &[(String, usize)], signatures: Vec<u64>) -> Self {
        assert_eq!(cores.len(), signatures.len());
        let contexts = cores
            .iter()
            .zip(signatures)
            .map(|((_, core), signature)| {
                let params = config::ant_config(*core).expect("task worker must be Ant").clone();
                let tls = params.tls.as_ref().unwrap();
                Arc::new(Context {
                    core: *core,
                    signature,
                    execution: Mutex::new(Execution {
                        code: Memory::new(0, params.code_bytes as usize),
                        tls: Memory::new(tls.base, tls.bytes as usize),
                        cpu: Hart::new(0, 0),
                    }),
                    params,
                    control: Mutex::new(Control {
                        dma: None,
                        descriptor: [0; 6],
                        fields: 0,
                        active: false,
                        done: false,
                        value: 0,
                    }),
                    ready: Condvar::new(),
                    cancelled: AtomicBool::new(false),
                })
            })
            .collect();
        let tss = cores.first().map(|(_, core)| {
            let p = config::tss_config(*core);
            Memory::new(p.base, p.bytes as usize)
        });
        let tss_range = tss.as_ref().map(|m| (m.base, m.bytes.len()));
        Self {
            contexts,
            tss: tss.map(Mutex::new),
            tss_range,
            group: Mutex::new(false),
            barrier: Mutex::new(Barrier {
                generation: 0,
                arrived: vec![false; cores.len()],
            }),
            barrier_ready: Condvar::new(),
            started: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            failure: Mutex::new(None),
        }
    }
    pub fn control(&self, operation: u32, a: u64, data: u64, owner: &Hart) -> u64 {
        let index = (a >> 32) as usize;
        let field = a as u32 as usize;
        if operation == 0 && field == 7 {
            return self.contexts.len() as u64;
        }
        if matches!(operation, 5 | 6) {
            let mut group = self.group.lock().unwrap();
            assert_eq!(*group, operation == 6);
            for context in &self.contexts {
                let control = context.control.lock().unwrap();
                assert!(!control.active && !control.done);
                if operation == 5 {
                    context.cancelled.store(false, Ordering::Release);
                }
            }
            *group = operation == 5;
            return 0;
        }
        if matches!(operation, 11 | 12) {
            assert_eq!(index, 0);
            assert!(!*self.group.lock().unwrap(), "TSS management requires released group");
            let mut memory = self.tss.as_ref().expect("tile has no TSS").lock().unwrap();
            assert_eq!(field % 8, 0);
            let address = memory.base.checked_add(field as u64).unwrap();
            return if operation == 11 {
                memory.read(address, 8).unwrap()
            } else {
                memory.write(address, 8, data).unwrap();
                0
            };
        }
        let group = *self.group.lock().unwrap();
        let context = &self.contexts[index];
        let mut control = context.control.lock().unwrap();
        if (7..=10).contains(&operation) {
            assert!(
                !control.active && !control.done,
                "private storage requires idle acknowledged context"
            );
            assert_eq!(field % 8, 0);
            let mut execution = context.execution.lock().unwrap();
            let memory = if operation <= 8 {
                &mut execution.code
            } else {
                &mut execution.tls
            };
            let address = memory.base.checked_add(field as u64).unwrap();
            return if operation % 2 == 1 {
                memory.read(address, 8).unwrap()
            } else {
                memory.write(address, 8, data).unwrap();
                0
            };
        }
        match operation {
            0 => match field {
                0 => context.signature,
                1 => {
                    1 | ((control.active as u64) << 1)
                        | ((control.done as u64) << 2)
                        | (((control.done && context.cancelled.load(Ordering::Acquire)) as u64) << 3)
                }
                2 => context.params.code_bytes as u64,
                3 => context.params.tls.as_ref().unwrap().base,
                4 => context.params.tls.as_ref().unwrap().bytes as u64,
                5 => self.tss_range.unwrap().0,
                6 => self.tss_range.unwrap().1 as u64,
                8 => {
                    assert!(control.done);
                    control.descriptor[0]
                }
                9 => {
                    assert!(control.done);
                    control.value
                }
                _ => panic!("invalid Ant query"),
            },
            1 => {
                assert!(!control.active && !control.done && field < 6);
                if field == 0 && context.params.task_bits < 64 {
                    assert!(data < (1 << context.params.task_bits));
                }
                control.descriptor[field] = data;
                control.fields |= 1 << field;
                0
            }
            2 => {
                assert!(group && !control.active && !control.done);
                assert_eq!(control.fields, 63);
                assert_eq!(control.descriptor[5], context.signature);
                let [_, entry, end, arg, stack, _] = control.descriptor;
                assert!(entry % 4 == 0 && end % 4 == 0 && entry < end && end <= context.params.code_bytes as u64);
                let tls = context.params.tls.as_ref().unwrap();
                assert!(arg >= tls.base && arg < tls.base + tls.bytes as u64);
                assert!(
                    stack % 16 == 0
                        && stack > context.params.tls.as_ref().unwrap().base
                        && stack
                            <= context.params.tls.as_ref().unwrap().base
                                + context.params.tls.as_ref().unwrap().bytes as u64
                );
                control.dma = Some((owner.csrs.clone(), owner.privilege));
                control.active = true;
                context.cancelled.store(false, Ordering::Release);
                control.fields = 0;
                context.ready.notify_one();
                0
            }
            3 => {
                assert!(control.done && !control.active);
                control.done = false;
                0
            }
            4 => {
                assert!(control.active);
                context.cancelled.store(true, Ordering::Release);
                drop(control);
                let waiting = self.barrier.lock().unwrap().arrived.iter().any(|a| *a);
                if waiting {
                    self.fail("Ant group barrier cancelled".into());
                }
                0
            }
            _ => panic!("invalid Ant control operation"),
        }
    }
}
