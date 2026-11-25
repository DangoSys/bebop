use generic::{
    configs,
    tasks::{Context, ThreadTimer, ThreadWorker},
    tlink::TLink,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{Builder, JoinHandle},
};

/// The hardware threads of a hart (arch: behind its TBus), each executing on a
/// host thread of its own, and the timing unit of the hart on another.
pub struct ThreadGroup {
    link: Arc<TLink>,
    threads: Vec<JoinHandle<()>>,
    reported: AtomicBool,
}
impl ThreadGroup {
    pub fn check(&self) -> Result<(), String> {
        if self.link.tasks.stopped.load(Ordering::Acquire) {
            if let Some(error) = self.link.tasks.failure.lock().unwrap().as_ref() {
                self.reported.store(true, Ordering::Release);
                return Err(error.clone());
            }
        }
        Ok(())
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.link.tasks.stop();
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                self.link.tasks.fail("Ant worker panicked outside execution".into());
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
    /// Runs every worker on a host thread of its own, the worker of the link's `index`-th
    /// thread `threads[index]`, and the timing unit of the hart with their timers.
    pub fn new(link: &Arc<TLink>, threads: Vec<(Box<dyn ThreadWorker>, Box<dyn ThreadTimer>)>) -> Result<Self, String> {
        let tasks = &link.tasks;
        assert!(
            !tasks.started.swap(true, Ordering::AcqRel),
            "Ant workers already started"
        );
        assert_eq!(threads.len(), tasks.contexts.len(), "one worker per thread");
        let mut group = ThreadGroup {
            link: Arc::clone(link),
            threads: Vec::new(),
            reported: AtomicBool::new(false),
        };
        let (workers, timers): (Vec<_>, Vec<_>) = threads.into_iter().unzip();
        for ((index, context), worker) in tasks.contexts.iter().enumerate().zip(workers) {
            let context = Arc::clone(context);
            let link = Arc::clone(link);
            let name = format!("ant-thread-{}", context.thread);
            let thread = Builder::new()
                .name(name)
                .spawn(move || {
                    configs::configure_thread(context.thread);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run_worker(&link, index, &context, worker);
                    }));
                    if let Err(panic) = result {
                        let message = panic
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| panic.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown panic");
                        link.tasks
                            .fail(format!("Ant thread {} failed: {message}", context.thread));
                    }
                })
                .map_err(|e| e.to_string())?;
            group.threads.push(thread);
        }
        let link = Arc::clone(link);
        let thread = Builder::new()
            .name(format!("hart-timing-{}", link.hart_id))
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    super::unit::run(&link, timers);
                }));
                if let Err(panic) = result {
                    let message = panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown panic");
                    link.tasks
                        .fail(format!("Timing unit of hart {} failed: {message}", link.hart_id));
                }
            })
            .map_err(|e| e.to_string())?;
        group.threads.push(thread);
        Ok(group)
    }
}

fn run_worker(link: &Arc<TLink>, index: usize, context: &Context, mut worker: Box<dyn ThreadWorker>) {
    let tasks = &link.tasks;
    let mut elapsed = std::time::Duration::ZERO;
    loop {
        let mut control = context.control.lock().unwrap();
        while !control.active && !tasks.stopped.load(Ordering::Acquire) {
            control = context.ready.wait(control).unwrap();
        }
        if tasks.stopped.load(Ordering::Acquire) {
            break;
        }
        let descriptor = control.descriptor;
        let start_cycle = control.start_cycle;
        let snapshot = control.dma.take().unwrap();
        drop(control);
        let started = std::time::Instant::now();
        worker.run(link, index, context, descriptor, start_cycle, &snapshot);
        elapsed += started.elapsed();
    }
    worker.finish(context.thread, elapsed);
}
