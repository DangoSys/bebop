use crate::configs;
use crate::guest::Transfer;
use crate::stores::{LocalMemory, Stores};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex,
};

/// The records a thread's execution can run ahead of the timing unit of its hart.
const RECORDS: usize = 1024;

/// What a hardware thread's execution reports to the timing unit of its hart: the cycles its
/// CPU ran since the previous record, then one event. The execution knows no cycle; the unit
/// replays the records into the thread's time.
pub struct Record {
    pub ant: u64,
    pub event: Event,
}

pub enum Event {
    /// The task starts at the hart's `cycle`.
    Begin { cycle: u64 },
    /// CPU instructions only.
    Run,
    /// A fence: its own four cycles, then the completion of the accelerator's submitted work.
    Fence,
    /// A custom instruction, with what its latency consists of: fixed cycles and a DMA.
    Custom {
        funct7: u8,
        xs1: u64,
        xs2: u64,
        ball: Option<u32>,
        cycles: u64,
        transfer: Option<Transfer>,
    },
    /// An access to the TSS, which the hart orders among its threads and its CPU.
    Shared,
    /// A read of the cycle CSR.
    Cycle,
    /// A DMA page fault: the task stalls until its owner resolves it.
    Fault(crate::riscv::Trap),
    /// The task ends with `value`.
    End { value: u64 },
}

/// A thread's records on their way to the timing unit, and the unit's reply to the last record
/// it replayed.
pub struct Records {
    pub queue: VecDeque<Record>,
    pushed: u64,
    replayed: u64,
    reply: (u64, u64),
    waiting: bool,
}

pub struct Control {
    pub start_cycle: u64,
    pub finish_cycle: u64,
    pub dma: Option<(crate::riscv::csr::Csrs, crate::riscv::Privilege)>,
    pub descriptor: [u64; 6],
    fields: u8,
    pub active: bool,
    pub done: bool,
    pub value: u64,
    // A DMA page fault the task is stalled on, until its owner resolves it.
    fault: Option<crate::riscv::Trap>,
    // The owner's resolution: retry or fail, and its cycle.
    resolution: Option<(bool, u64)>,
}
pub struct Completion {
    pub clint: Arc<crate::clint::Clint>,
    /// The owner CPU waits for a completion (WAIT_COMPLETION) under this lock.
    pub waiting: Mutex<bool>,
    pub ready: Condvar,
    /// Counts what can let the timing unit go on: a record at the head of a queue, a
    /// resolution, a cancellation, a stop.
    pub events: AtomicU64,
    asleep: Mutex<bool>,
    awake: Condvar,
}

impl Completion {
    fn wake(&self) {
        self.events.fetch_add(1, Ordering::SeqCst);
        if std::mem::take(&mut *self.asleep.lock().unwrap()) {
            self.awake.notify_one();
        }
        self.clint.notify_shared();
    }

    /// The timing unit sleeps until an event follows `events`, its sample of the count.
    pub fn sleep(&self, events: u64) {
        let mut asleep = self.asleep.lock().unwrap();
        while self.events.load(Ordering::SeqCst) == events {
            *asleep = true;
            asleep = self.awake.wait(asleep).unwrap();
        }
    }
}

pub struct Context {
    pub shared_blocked: AtomicBool,
    pub progress: Arc<AtomicU64>,
    not_done_before: AtomicU64,
    cpu_waiting: AtomicBool,
    completion: Arc<Completion>,
    pub thread: usize,
    signature: u64,
    pub params: configs::AntConfig,
    pub stores: Arc<Mutex<Stores>>,
    pub control: Mutex<Control>,
    pub ready: Condvar,
    pub cancelled: AtomicBool,
    pub records: Mutex<Records>,
    replayed: Condvar,
}

/// The timing unit's side of a thread's task port: it publishes the thread's time, its stalls
/// and its completion to the hart's CPU, and answers the records the thread waits for.
impl Context {
    /// The task stalls on a DMA page fault until its owner resolves it (operation 25).
    pub fn stall(&self, trap: crate::riscv::Trap) {
        self.block_shared(true);
        let mut control = self.control.lock().unwrap();
        control.fault = Some(trap);
        self.ready.notify_all();
        drop(control);
        // A stalled task ends the owner's WAIT_COMPLETION like a completion.
        let _waiting = self.completion.waiting.lock().unwrap();
        self.completion.ready.notify_one();
    }

    /// The owner resolved the stall or cancelled the task.
    pub fn resolved(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.control.lock().unwrap().resolution.is_some()
    }

    /// Ends the stall: the owner's cycle at a retry, None for a cancelled task; a failed fault
    /// ends the run.
    pub fn resume(&self) -> Option<u64> {
        let mut control = self.control.lock().unwrap();
        let trap = control.fault.take().unwrap();
        let (retry, cycle) = control.resolution.take()?;
        assert!(retry, "Ant DMA page fault at 0x{:x} failed by its owner", trap.value);
        Some(cycle)
    }

    pub fn advance(&self, cycle: u64, completion: u64) {
        self.progress.store(cycle, Ordering::SeqCst);
        self.not_done_before.fetch_max(completion, Ordering::SeqCst);
        self.completion.clint.notify_shared();
        // Pair the condition and notification so a CPU cannot miss progress
        // between checking the clock and sleeping on the task mutex.
        if self.cpu_waiting.load(Ordering::SeqCst) {
            let _control = self.control.lock().unwrap();
            self.ready.notify_all();
        }
    }

    pub fn block_shared(&self, blocked: bool) {
        self.shared_blocked.store(blocked, Ordering::Release);
        self.completion.clint.notify_shared();
    }

    /// The task is done at `cycle` with `value`.
    pub fn finish(&self, cycle: u64, value: u64) {
        let mut control = self.control.lock().unwrap();
        control.finish_cycle = cycle;
        self.progress.store(u64::MAX, Ordering::Release);
        control.value = if self.cancelled.load(Ordering::Acquire) {
            0
        } else {
            value
        };
        control.active = false;
        control.done = true;
        self.ready.notify_all();
        drop(control);
        self.completion.clint.notify_shared();
        let _waiting = self.completion.waiting.lock().unwrap();
        self.completion.ready.notify_all();
    }

    /// The record taken from the queue is replayed: `reply` answers a thread that waits for it.
    pub fn replied(&self, reply: (u64, u64)) {
        let mut records = self.records.lock().unwrap();
        records.replayed += 1;
        records.reply = reply;
        if std::mem::take(&mut records.waiting) {
            self.replayed.notify_all();
        }
    }
}
pub struct Tasks {
    pub contexts: Vec<Arc<Context>>,
    pub tss: Option<Mutex<LocalMemory>>,
    pub tss_range: Option<(u64, usize)>,
    group: Mutex<bool>,
    pub completion: Arc<Completion>,
    pub started: AtomicBool,
    pub stopped: AtomicBool,
    pub failure: Mutex<Option<String>>,
}
impl Tasks {
    pub fn new(threads: &[usize], clint: Arc<crate::clint::Clint>) -> Self {
        for (slot, thread) in threads.iter().enumerate() {
            assert_eq!(
                configs::thread_local_id(*thread),
                slot,
                "task-port IDs must match explicitly connected slots"
            );
        }
        let completion = Arc::new(Completion {
            clint,
            waiting: Mutex::new(false),
            ready: Condvar::new(),
            events: AtomicU64::new(0),
            asleep: Mutex::new(false),
            awake: Condvar::new(),
        });
        let contexts = threads
            .iter()
            .map(|thread| {
                let signature = configs::thread_signature(*thread);
                let params = configs::ant_config(*thread).expect("task worker must be Ant").clone();
                Arc::new(Context {
                    shared_blocked: AtomicBool::new(false),
                    progress: Arc::new(AtomicU64::new(u64::MAX)),
                    not_done_before: AtomicU64::new(0),
                    cpu_waiting: AtomicBool::new(false),
                    completion: Arc::clone(&completion),
                    thread: *thread,
                    signature,
                    stores: Arc::new(Mutex::new(Stores::new(&params))),
                    params,
                    control: Mutex::new(Control {
                        start_cycle: 0,
                        finish_cycle: 0,
                        dma: None,
                        descriptor: [0; 6],
                        fields: 0,
                        active: false,
                        done: false,
                        value: 0,
                        fault: None,
                        resolution: None,
                    }),
                    ready: Condvar::new(),
                    cancelled: AtomicBool::new(false),
                    records: Mutex::new(Records {
                        queue: VecDeque::new(),
                        pushed: 0,
                        replayed: 0,
                        reply: (0, 0),
                        waiting: false,
                    }),
                    replayed: Condvar::new(),
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
            completion,
            started: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            failure: Mutex::new(None),
        }
    }

    /// Thread `index` queues a record for the timing unit; it waits while its queue is full.
    pub fn record(&self, index: usize, record: Record) {
        let context = &self.contexts[index];
        let mut records = context.records.lock().unwrap();
        while records.queue.len() == RECORDS && !self.stopped.load(Ordering::Acquire) {
            records.waiting = true;
            records = context.replayed.wait(records).unwrap();
        }
        records.queue.push_back(record);
        records.pushed += 1;
        // The unit sees the heads of the queues: only a record that becomes one is news.
        if records.queue.len() == 1 {
            drop(records);
            self.completion.wake();
        }
    }

    /// Thread `index` queues a record and waits until the timing unit has replayed it: the
    /// unit's reply, or None once the threads stop.
    pub fn sync(&self, index: usize, record: Record) -> Option<(u64, u64)> {
        self.record(index, record);
        let context = &self.contexts[index];
        let mut records = context.records.lock().unwrap();
        while records.replayed != records.pushed && !self.stopped.load(Ordering::Acquire) {
            records.waiting = true;
            records = context.replayed.wait(records).unwrap();
        }
        (!self.stopped.load(Ordering::Acquire)).then_some(records.reply)
    }

    /// What WAIT_COMPLETION sees: the earliest cycle a task is done or stalled at with the
    /// result bits, and the earliest cycle a running task can complete at.
    fn completions(&self) -> (Option<(u64, u64)>, Option<u64>) {
        let mut first = None;
        let mut next = None;
        let mut result = 0;
        for context in &self.contexts {
            let control = context.control.lock().unwrap();
            if control.active {
                let progress = context
                    .progress
                    .load(Ordering::SeqCst)
                    .max(context.not_done_before.load(Ordering::SeqCst));
                next = Some(next.map_or(progress, |value: u64| value.min(progress)));
                // A stalled task's clock stays at the cycle of its fault.
                if control.fault.is_some() && control.resolution.is_none() {
                    result |= 2;
                    first = Some(first.map_or(progress, |value: u64| value.min(progress)));
                }
            }
            if control.done {
                result |= 1;
                first = Some(first.map_or(control.finish_cycle, |value: u64| value.min(control.finish_cycle)));
            }
        }
        (first.map(|cycle| (cycle, result)), next)
    }

    /// The cycle the owner CPU `hart` has reached, for the timing unit, which sampled the
    /// CPU's clock as `clock` before it looked at the threads. A CPU that waits for a
    /// completion follows its threads: the unit moves its clock to the earliest cycle it can
    /// return at, so that no CPU waits for a host thread to finish a whole task. Returns the
    /// cycle and whether the CPU follows.
    pub fn owner_time(&self, hart: usize, clock: u64) -> (u64, bool) {
        let waiting = self.completion.waiting.lock().unwrap();
        if !*waiting {
            return (clock, false);
        }
        let (first, next) = self.completions();
        if let Some(cycle) = first.map(|(cycle, _)| cycle).or(next) {
            self.completion.clint.advance_to(hart, cycle);
        }
        (self.completion.clint.shared_time(hart), true)
    }

    /// The hart's CPU reaches shared state at `cycle` once every running thread has reached it.
    pub fn shared_access(&self, cycle: u64, clint: &crate::clint::Clint) -> bool {
        let running = clint.wait_shared(|| {
            self.stopped.load(Ordering::Acquire)
                || self.contexts.iter().all(|context| {
                    context.shared_blocked.load(Ordering::Acquire)
                        || context.cancelled.load(Ordering::Acquire)
                        || context.progress.load(Ordering::Acquire) >= cycle
                })
        });
        running && !self.stopped.load(Ordering::Acquire)
    }

    pub fn control(
        &self,
        operation: u32,
        a: u64,
        data: u64,
        owner: &crate::riscv::registers::Registers,
        clint: &crate::clint::Clint,
        cycle: &mut u64,
    ) -> u64 {
        clint.advance_to(owner.id as usize, *cycle);
        if (7..=12).contains(&operation) {
            assert!(self.shared_access(*cycle, clint));
        }
        let index = (a >> 32) as usize;
        let field = a as u32 as usize;
        if operation == 0 && field == 7 {
            return self.contexts.len() as u64;
        }
        // WAIT_COMPLETION: returns once a task is done (bit 0) or stalled on a
        // DMA page fault (bit 1), at the earliest such cycle, or 0 when none is owned.
        if operation == 0 && field == 10 {
            let mut waiting = self.completion.waiting.lock().unwrap();
            loop {
                let (first, next) = self.completions();
                if first.is_none() && next.is_some() && !self.stopped.load(Ordering::Acquire) {
                    // The timing unit moves the clock of a CPU that waits here (`owner_time`).
                    if !std::mem::replace(&mut *waiting, true) {
                        self.completion.wake();
                    }
                    waiting = self.completion.ready.wait(waiting).unwrap();
                    continue;
                }
                if std::mem::take(&mut *waiting) {
                    self.completion.wake();
                }
                let (earliest, result) = first.unwrap_or((0, 0));
                *cycle = (*cycle).max(earliest);
                return result;
            }
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
            let mut memory = self.tss.as_ref().expect("hart has no TSS").lock().unwrap();
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
            let mut execution = context.stores.lock().unwrap();
            let local = if operation <= 8 {
                &mut execution.code
            } else {
                &mut execution.tls
            };
            let mut shared = self.tss.as_ref().expect("hart has no TSS").lock().unwrap();
            assert!(field.checked_add(bytes).unwrap() <= local.bytes.len());
            assert!(shared_offset.checked_add(bytes).unwrap() <= shared.bytes.len());
            if operation % 2 == 0 {
                local.bytes[field..field + bytes].copy_from_slice(&shared.bytes[shared_offset..shared_offset + bytes]);
            } else {
                shared.bytes[shared_offset..shared_offset + bytes].copy_from_slice(&local.bytes[field..field + bytes]);
            }
            return 0;
        }
        // The status and the page fault of a running task are those of the
        // hart's cycle: wait for the task to reach it, unless it is stalled.
        if (operation == 0 && field == 1 || operation == 24) && control.active {
            let limit = cycle.saturating_sub(64);
            let progress = || {
                context
                    .progress
                    .load(Ordering::SeqCst)
                    .max(context.not_done_before.load(Ordering::SeqCst))
            };
            if progress() < limit {
                context.cpu_waiting.store(true, Ordering::SeqCst);
                while control.active
                    && control.fault.is_none()
                    && progress() < limit
                    && !self.stopped.load(Ordering::Acquire)
                {
                    control = context.ready.wait(control).unwrap();
                }
                context.cpu_waiting.store(false, Ordering::SeqCst);
            }
        }
        if control.done && ((operation == 0 && matches!(field, 1 | 8 | 9)) || operation == 3) {
            *cycle = (*cycle).max(control.finish_cycle);
        }
        match operation {
            0 => match field {
                0 => context.signature,
                1 => {
                    1 | ((control.active as u64) << 1)
                        | ((control.done as u64) << 2)
                        | (((control.done && context.cancelled.load(Ordering::Acquire)) as u64) << 3)
                        | (((control.fault.is_some() && control.resolution.is_none()) as u64) << 4)
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
                control.start_cycle = *cycle;
                context.not_done_before.store(*cycle, Ordering::SeqCst);
                context.progress.store(*cycle, Ordering::Release);
                context.shared_blocked.store(false, Ordering::Release);
                control.active = true;
                context.cancelled.store(false, Ordering::Release);
                control.fields = 0;
                context.ready.notify_one();
                clint.notify_shared();
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
                self.completion.wake();
                context.ready.notify_all();
                0
            }
            // The page fault the task is stalled on: bit 63 stalled, bit 62 a
            // store, bits 61:0 the virtual address.
            24 => control
                .fault
                .filter(|_| control.resolution.is_none())
                .map_or(0, |trap| {
                    1 << 63 | ((trap.cause == 15) as u64) << 62 | trap.value & ((1 << 62) - 1)
                }),
            // Retry (1) or fail (0) the stalled DMA.
            25 => {
                assert!(
                    control.fault.is_some() && control.resolution.is_none() && data <= 1,
                    "Ant DMA fault resolved without a stalled DMA"
                );
                control.resolution = Some((data == 1, *cycle));
                context.ready.notify_all();
                self.completion.wake();
                0
            }
            _ => panic!("invalid Ant control operation"),
        }
    }
}

impl Tasks {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        for context in &self.contexts {
            context.cancelled.store(true, Ordering::Release);
        }
        self.completion.wake();
        // Pair notification with the waiter's mutex to prevent a lost wakeup.
        for context in &self.contexts {
            let _control = context.control.lock().unwrap();
            context.ready.notify_all();
            let _records = context.records.lock().unwrap();
            context.replayed.notify_all();
        }
        let _waiting = self.completion.waiting.lock().unwrap();
        self.completion.ready.notify_all();
    }

    pub fn fail(&self, error: String) {
        self.failure.lock().unwrap().get_or_insert(error);
        self.stop();
    }
}

/// A hardware thread's execution of the tasks its hart gives it. Thread-level parallelism runs
/// each worker on a host thread of its own, ahead of the thread's time: the worker reports what
/// it executes in records and waits for the timing unit only where it observes time or state
/// the hart shares.
pub trait ThreadWorker: Send {
    /// Runs the task `descriptor`, started at `start_cycle`, with the issuer's DMA view.
    fn run(
        &mut self,
        link: &Arc<crate::tlink::TLink>,
        index: usize,
        context: &Context,
        descriptor: [u64; 6],
        start_cycle: u64,
        dma: &(crate::riscv::csr::Csrs, crate::riscv::Privilege),
    );
    /// The thread's work is over: release what it holds and report its profile.
    fn finish(&mut self, thread: usize, elapsed: std::time::Duration);
}

/// A hardware thread's time, which the timing unit of its hart advances by replaying the
/// records of the thread's execution.
pub trait ThreadTimer: Send {
    /// The cycle the hart accepts `record` at when it takes the hart's shared memory path;
    /// None for a record local to the thread.
    fn accepted(&self, record: &Record) -> Option<u64>;
    /// Replays `record`: the reply to a thread that waits for it.
    fn replay(&mut self, link: &crate::tlink::TLink, record: &Record) -> (u64, u64);
    /// The thread retries its stalled instruction at the owner's `cycle`.
    fn resume(&mut self, cycle: u64);
    /// The thread's cycle and the cycle its submitted work completes.
    fn cycles(&self) -> (u64, u64);
}
