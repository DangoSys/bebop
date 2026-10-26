use super::tasks::{Context, Tasks};
use crate::core::riscv::hart::Hart;
use crate::{configs, core::CoreState, thread::ThreadModel};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{Builder, JoinHandle},
};

pub(super) struct Barrier {
    pub(super) generation: u64,
    pub(super) arrived: Vec<bool>,
}
/// The hardware threads of a core (arch: behind its TBus), each running on a
/// host thread of its own.
pub struct ThreadGroup {
    core: Arc<CoreState>,
    threads: Vec<JoinHandle<()>>,
    reported: AtomicBool,
}
impl ThreadGroup {
    pub fn check(&self) -> Result<(), String> {
        if self.core.tasks.stopped.load(Ordering::Acquire) {
            if let Some(error) = self.core.tasks.failure.lock().unwrap().as_ref() {
                self.reported.store(true, Ordering::Release);
                return Err(error.clone());
            }
        }
        Ok(())
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.core.tasks.stop();
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                self.core.tasks.fail("Ant worker panicked outside execution".into());
            }
        }
        self.check()
    }
}
impl Drop for ThreadGroup {
    fn drop(&mut self) {
        let reported = self.reported.load(Ordering::Acquire);
        if let Err(error) = self.shutdown() {
            if !reported {
                panic!("Ant worker shutdown: {error}");
            }
        }
    }
}
impl ThreadGroup {
    pub fn new(core: &Arc<CoreState>, options: &crate::ModelOptions) -> Result<Self, String> {
        let tasks = &core.tasks;
        let directory = &options.log_dir;
        let trace = options.trace.clone();
        let profile = options.profile;
        assert!(
            !tasks.started.swap(true, Ordering::AcqRel),
            "Ant workers already started"
        );
        let mut workers = ThreadGroup {
            core: Arc::clone(core),
            threads: Vec::new(),
            reported: AtomicBool::new(false),
        };
        for (index, context) in tasks.contexts.iter().enumerate() {
            let dir = directory.join(format!("ant-{index}"));
            let model = ThreadModel::new(core, context.thread, &dir, trace.clone(), profile)?;
            let context = Arc::clone(context);
            let core = Arc::clone(core);
            let name = format!("ant-thread-{}", context.thread);
            let thread = Builder::new()
                .name(name)
                .spawn(move || {
                    configs::configure_thread(context.thread);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        core.tasks.run_worker(index, &context, &core, model);
                    }));
                    if let Err(panic) = result {
                        let message = panic
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| panic.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown panic");
                        core.tasks
                            .fail(format!("Ant thread {} failed: {message}", context.thread));
                    }
                })
                .map_err(|e| e.to_string())?;
            workers.threads.push(thread);
        }
        Ok(workers)
    }
}

impl Tasks {
    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        // Pair notification with the waiter's mutex to prevent a lost wakeup.
        for context in &self.contexts {
            match context.control.lock() {
                Ok(_control) => context.ready.notify_all(),
                Err(_) => {
                    self.failure
                        .lock()
                        .unwrap()
                        .get_or_insert_with(|| format!("Ant thread {} control mutex poisoned", context.thread));
                    context.ready.notify_all();
                }
            }
        }
        match self.barrier.lock() {
            Ok(_barrier) => self.barrier_ready.notify_all(),
            Err(_) => {
                self.failure
                    .lock()
                    .unwrap()
                    .get_or_insert_with(|| "Ant group barrier mutex poisoned".into());
                self.barrier_ready.notify_all();
            }
        }
    }

    pub(super) fn fail(&self, error: String) {
        self.failure.lock().unwrap().get_or_insert(error);
        self.stop();
    }
    pub(crate) fn rendezvous(&self, index: usize) -> bool {
        let mut barrier = self.barrier.lock().unwrap();
        if self.contexts.iter().any(|c| c.cancelled.load(Ordering::Acquire)) {
            drop(barrier);
            self.fail("Ant group barrier cancelled".into());
            return false;
        }
        let generation = barrier.generation;
        assert!(!barrier.arrived[index], "duplicate Ant barrier arrival");
        barrier.arrived[index] = true;
        if barrier.arrived.iter().all(|arrived| *arrived) {
            barrier.arrived.fill(false);
            barrier.generation += 1;
            self.barrier_ready.notify_all();
        } else {
            while barrier.generation == generation && !self.stopped.load(Ordering::Acquire) {
                barrier = self.barrier_ready.wait(barrier).unwrap();
            }
        }
        !self.stopped.load(Ordering::Acquire)
    }
    fn run_worker(&self, index: usize, context: &Context, core: &Arc<CoreState>, model: ThreadModel) {
        let ThreadModel { ant, mut acc } = model;
        loop {
            let mut control = context.control.lock().unwrap();
            while !control.active && !self.stopped.load(Ordering::Acquire) {
                control = context.ready.wait(control).unwrap();
            }
            if self.stopped.load(Ordering::Acquire) {
                break;
            }
            let descriptor = control.descriptor;
            let snapshot = control.dma.take().unwrap();
            drop(control);
            let outcome = {
                let mut execution = ant.lock().unwrap();
                execution.cpu = Hart::new(0, descriptor[1]);
                execution.cpu.set_register(2, descriptor[4]);
                let tls_base = execution.tls.base;
                execution.cpu.set_register(4, tls_base);
                execution.cpu.set_register(10, descriptor[3]);
                acc.trace.task_event("begin", descriptor[0], 0);
                let (outcome, steps) = if acc.trace.itrace {
                    execution.execute::<true>(self, index, context, &mut acc, core, descriptor[2], &snapshot)
                } else {
                    execution.execute::<false>(self, index, context, &mut acc, core, descriptor[2], &snapshot)
                };
                acc.trace.task_event("end", descriptor[0], steps);
                outcome
            };
            if self.stopped.load(Ordering::Acquire) {
                break;
            }
            let mut control = context.control.lock().unwrap();
            control.value = if context.cancelled.load(Ordering::Acquire) {
                0
            } else {
                outcome
            };
            control.active = false;
            control.done = true;
        }
        acc.finish_bank_frees();
    }
}
