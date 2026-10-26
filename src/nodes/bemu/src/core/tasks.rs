use super::threads::Barrier;
use crate::configs;
use crate::thread::ant::{Ant, LocalMemory};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};

pub(crate) struct Control {
    pub(crate) dma: Option<(crate::core::riscv::csr::Csrs, crate::core::riscv::Privilege)>,
    pub(crate) descriptor: [u64; 6],
    fields: u8,
    pub(crate) active: bool,
    pub(crate) done: bool,
    pub(crate) value: u64,
}
pub(crate) struct Context {
    pub(crate) thread: usize,
    signature: u64,
    pub(crate) params: configs::AntConfig,
    pub(crate) execution: Arc<Mutex<Ant>>,
    pub(crate) control: Mutex<Control>,
    pub(crate) ready: Condvar,
    pub(crate) cancelled: AtomicBool,
}
pub(crate) struct Tasks {
    pub(crate) contexts: Vec<Arc<Context>>,
    pub(crate) tss: Option<Mutex<LocalMemory>>,
    pub(crate) tss_range: Option<(u64, usize)>,
    group: Mutex<bool>,
    pub(super) barrier: Mutex<Barrier>,
    pub(crate) barrier_ready: Condvar,
    pub(crate) started: AtomicBool,
    pub(crate) stopped: AtomicBool,
    pub(crate) failure: Mutex<Option<String>>,
}
impl Tasks {
    pub fn new(threads: &[usize]) -> Self {
        for (slot, thread) in threads.iter().enumerate() {
            assert_eq!(
                configs::thread_local_id(*thread),
                slot,
                "task-port IDs must match explicitly connected slots"
            );
        }
        let contexts = threads
            .iter()
            .map(|thread| {
                let signature = configs::thread_signature(*thread);
                let params = configs::ant_config(*thread).expect("task worker must be Ant").clone();
                Arc::new(Context {
                    thread: *thread,
                    signature,
                    execution: Arc::new(Mutex::new(Ant::new(&params))),
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
        let tss = threads.first().map(|thread| {
            let p = configs::tss_config(*thread);
            LocalMemory::new(p.base, p.bytes as usize)
        });
        let tss_range = tss.as_ref().map(|m| (m.base, m.bytes.len()));
        Self {
            contexts,
            tss: tss.map(Mutex::new),
            tss_range,
            group: Mutex::new(false),
            barrier: Mutex::new(Barrier {
                generation: 0,
                arrived: vec![false; threads.len()],
            }),
            barrier_ready: Condvar::new(),
            started: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            failure: Mutex::new(None),
        }
    }
    pub fn control(&self, operation: u32, a: u64, data: u64, owner: &crate::core::riscv::hart::Hart) -> u64 {
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
            let mut memory = self.tss.as_ref().expect("core has no TSS").lock().unwrap();
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
                "TSS transfer requires idle acknowledged context"
            );
            let shared_offset = data as u32 as usize;
            let bytes = (data >> 32) as usize;
            assert!(bytes != 0 && (field | shared_offset | bytes) % 8 == 0);
            let mut execution = context.execution.lock().unwrap();
            let local = if operation <= 8 {
                &mut execution.code
            } else {
                &mut execution.tls
            };
            let mut shared = self.tss.as_ref().expect("core has no TSS").lock().unwrap();
            assert!(field.checked_add(bytes).unwrap() <= local.bytes.len());
            assert!(shared_offset.checked_add(bytes).unwrap() <= shared.bytes.len());
            if operation % 2 == 0 {
                local.bytes[field..field + bytes].copy_from_slice(&shared.bytes[shared_offset..shared_offset + bytes]);
            } else {
                shared.bytes[shared_offset..shared_offset + bytes].copy_from_slice(&local.bytes[field..field + bytes]);
            }
            return 0;
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
