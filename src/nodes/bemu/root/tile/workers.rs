use super::{
    tasks::{Context, Memory, Tasks},
    Tile,
};
use crate::{
    accel, config,
    root::{mmu::GuestAccess, platform::Port},
    trace::TraceConfig,
};
use rvsim::{
    bus::{Bus, BusError, HartBus, Width},
    hart::{Hart, Step},
    input::Inputs,
    mmu::Mmu,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

struct LocalBus<'a> {
    code: &'a Memory,
    tls: &'a mut Memory,
    tss: &'a Mutex<Memory>,
}
impl Bus for LocalBus<'_> {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        let n = width as usize;
        if self.code.offset(address, n).is_some() {
            self.code.read(address, n)
        } else if self.tls.offset(address, n).is_some() {
            self.tls.read(address, n)
        } else {
            self.tss.lock().unwrap().read(address, n)
        }
    }
    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        if self.tls.offset(address, width as usize).is_some() {
            self.tls.write(address, width as usize, value)
        } else {
            self.tss.lock().unwrap().write(address, width as usize, value)
        }
    }
    fn compare_exchange(&mut self, _: u64, _: Width, _: u64, _: u64) -> Result<u64, BusError> {
        panic!("Ant has no atomic instructions")
    }
}
impl HartBus for LocalBus<'_> {
    fn load_reserved(&mut self, _: u64, _: u64, _: Width) -> Result<u64, BusError> {
        panic!("Ant has no LR")
    }
    fn store_conditional(&mut self, _: u64, _: u64, _: Width, _: u64) -> Result<bool, BusError> {
        panic!("Ant has no SC")
    }
}
fn legal(instruction: u32) -> bool {
    let op = instruction & 127;
    let f3 = (instruction >> 12) & 7;
    let f7 = instruction >> 25;
    let word = op & 8 != 0;
    match op {
        0x37 | 0x17 | 0x6f | 0x7b => true,
        0x67 => f3 == 0,
        0x63 => f3 == 0 || f3 == 1 || f3 >= 4,
        0x03 => f3 <= 6,
        0x23 => f3 <= 3,
        0x0f => f3 == 0,
        0x13 | 0x1b => {
            let operation = !word || matches!(f3, 0 | 1 | 5);
            let shift = if word {
                f7 == 0 || (f3 == 5 && f7 == 32)
            } else {
                instruction >> 26 == 0 || (f3 == 5 && instruction >> 26 == 16)
            };
            operation && (!matches!(f3, 1 | 5) || shift)
        }
        0x33 | 0x3b => {
            if f7 == 1 {
                !word || f3 == 0 || f3 >= 4
            } else {
                (!word || matches!(f3, 0 | 1 | 5)) && (f7 == 0 || (f7 == 32 && matches!(f3, 0 | 5)))
            }
        }
        _ => false,
    }
}
pub(super) struct Execution {
    pub(super) code: Memory,
    pub(super) tls: Memory,
    pub(super) cpu: Hart,
}
pub(super) struct Barrier {
    pub(super) generation: u64,
    pub(super) arrived: Vec<bool>,
}
pub(crate) struct Workers {
    tile: Arc<Tile>,
    threads: Vec<JoinHandle<()>>,
    reported: AtomicBool,
}
impl Workers {
    pub fn check(&self) -> Result<(), String> {
        if self.tile.tasks.stopped.load(Ordering::Acquire) {
            if let Some(error) = self.tile.tasks.failure.lock().unwrap().as_ref() {
                self.reported.store(true, Ordering::Release);
                return Err(error.clone());
            }
        }
        Ok(())
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.tile.tasks.stop();
        for thread in self.threads.drain(..) {
            if thread.join().is_err() {
                self.tile.tasks.fail("Ant worker panicked outside execution".into());
            }
        }
        self.check()
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        let reported = self.reported.load(Ordering::Acquire);
        if let Err(error) = self.shutdown() {
            if !reported {
                panic!("Ant worker shutdown: {error}");
            }
        }
    }
}
impl Tasks {
    pub fn start(
        &self,
        tile: &Arc<Tile>,
        directory: &Path,
        trace: TraceConfig,
        profile: bool,
    ) -> Result<Workers, String> {
        assert!(
            !self.started.swap(true, Ordering::AcqRel),
            "Ant workers already started"
        );
        let mut workers = Workers {
            tile: Arc::clone(tile),
            threads: Vec::new(),
            reported: AtomicBool::new(false),
        };
        for (index, context) in self.contexts.iter().enumerate() {
            config::configure_core(context.core);
            let dir = directory.join(format!("ant-{index}"));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let endpoint = tile.endpoint_ids.iter().position(|id| *id == context.core).unwrap();
            let accelerator = accel::State::new(
                &dir,
                trace.clone(),
                profile,
                context.core,
                Some(endpoint),
                Some(Arc::clone(tile)),
            )?;
            let context = Arc::clone(context);
            let tile = Arc::clone(tile);
            let name = format!("ant-core-{}", context.core);
            let thread = thread::Builder::new()
                .name(name)
                .spawn(move || {
                    config::configure_core(context.core);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        tile.tasks.run_worker(index, &context, &tile, accelerator);
                    }));
                    if let Err(panic) = result {
                        let message = panic
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| panic.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown panic");
                        tile.tasks.fail(format!("Ant core {} failed: {message}", context.core));
                    }
                })
                .map_err(|e| e.to_string())?;
            workers.threads.push(thread);
        }
        Ok(workers)
    }
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
                        .get_or_insert_with(|| format!("Ant core {} control mutex poisoned", context.core));
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
    fn rendezvous(&self, index: usize) -> bool {
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
    fn run_worker(&self, index: usize, context: &Context, tile: &Arc<Tile>, mut accelerator: accel::State) {
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
                let mut execution = context.execution.lock().unwrap();
                execution.cpu = Hart::new(0, descriptor[1]);
                execution.cpu.set_register(2, descriptor[4]);
                let tls_base = execution.tls.base;
                execution.cpu.set_register(4, tls_base);
                execution.cpu.set_register(10, descriptor[3]);
                self.execute_job(
                    index,
                    context,
                    &mut execution,
                    &mut accelerator,
                    tile,
                    descriptor[2],
                    &snapshot,
                )
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
        accelerator.finish_bank_frees();
    }
    fn execute_job(
        &self,
        index: usize,
        context: &Context,
        execution: &mut Execution,
        accelerator: &mut accel::State,
        tile: &Arc<Tile>,
        end: u64,
        snapshot: &(rvsim::csr::Csrs, rvsim::Privilege),
    ) -> u64 {
        loop {
            if self.stopped.load(Ordering::Acquire) || context.cancelled.load(Ordering::Acquire) {
                return 0;
            }
            let pc = execution.cpu.pc;
            assert!(pc % 4 == 0 && pc + 4 <= end, "Ant fetch outside loaded code");
            let instruction = execution.code.read(pc, 4).unwrap() as u32;
            if instruction == 0x73 {
                assert_eq!(execution.cpu.register(17), 0, "unsupported Ant ecall");
                return execution.cpu.register(10);
            }
            let opcode = instruction & 127;
            assert!(legal(instruction), "unsupported Ant instruction {instruction:08x}");
            if matches!(opcode, 0x03 | 0x23) {
                let imm = if opcode == 3 {
                    (instruction as i32) >> 20
                } else {
                    (((instruction >> 25) << 5 | (instruction >> 7) & 31) as i32) << 20 >> 20
                };
                let address = execution
                    .cpu
                    .register(((instruction >> 15) & 31) as usize)
                    .wrapping_add(imm as u64);
                let width = 1usize << ((instruction >> 12) & 3);
                assert_eq!(address % width as u64, 0);
                assert!(
                    execution.tls.offset(address, width).is_some()
                        || self.tss_range.is_some_and(|(base, bytes)| address >= base
                            && address
                                .checked_add(width as u64)
                                .is_some_and(|last| last <= base + bytes as u64)),
                    "Ant ordinary memory outside TLS/TSS"
                );
            }
            let mmu = Mmu::default();
            let mut bus = LocalBus {
                code: &execution.code,
                tls: &mut execution.tls,
                tss: self.tss.as_ref().unwrap(),
            };
            let mut step = execution.cpu.step(&mut bus, &mmu, Inputs::default());
            if let Step::Custom(command) = step {
                assert_eq!(opcode, 0x7b);
                let (csrs, privilege) = snapshot;
                let privilege = if *privilege == rvsim::Privilege::Machine && csrs.satp != rvsim::mmu::Satp::Bare {
                    rvsim::Privilege::Supervisor
                } else {
                    *privilege
                };
                let translation = rvsim::mmu::TranslationContext {
                    privilege,
                    satp: csrs.satp,
                    mstatus: csrs.read(0x300, rvsim::Privilege::Machine, 0, 0, 0).unwrap(),
                    pmp: &csrs.pmp,
                };
                let memory = GuestAccess::new(
                    Port {
                        devices: &tile.platform,
                        memory: &tile.memory,
                    },
                    &mmu,
                    translation,
                );
                let value = accel::execute(
                    accelerator,
                    memory,
                    (instruction >> 25) as u8,
                    if instruction & (1 << 13) != 0 { command.rs1 } else { 0 },
                    if instruction & (1 << 12) != 0 { command.rs2 } else { 0 },
                    pc,
                )
                .expect("Ant NPU access failed");
                step = execution
                    .cpu
                    .complete_custom(Ok((instruction & (1 << 14) != 0).then_some(value)));
            }
            assert!(
                matches!(step, Step::Retired { .. }),
                "Ant execution failed at {pc:x}: {step:?}"
            );
            if accelerator.barrier_hit {
                if !self.rendezvous(index) {
                    return 0;
                }
                accelerator.barrier_hit = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::root::chip::Chip;

    fn tile() -> Arc<Tile> {
        let topology = config::tile_topology(1);
        let signatures = topology.worker_cores.iter()
            .map(|(_, core)| config::core_signature(*core)).collect();
        Tile::new(&Chip::new(16 << 20, config::hart_capacity()), &topology, signatures)
    }

    #[test]
    fn unreported_join_failure_panics_after_every_thread_is_joined() {
        let joined = Arc::new(AtomicBool::new(false));
        let completed = Arc::clone(&joined);
        let workers = Workers {
            tile: tile(),
            threads: vec![thread::spawn(|| panic!("outside execution")),
                          thread::spawn(move || completed.store(true, Ordering::Release))],
            reported: AtomicBool::new(false),
        };
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(workers)));
        assert!(failure.is_err());
        assert!(joined.load(Ordering::Acquire));
    }

    #[test]
    fn poisoned_control_is_a_shutdown_error_without_recovering_state() {
        let tile = tile();
        let context = Arc::clone(&tile.tasks.contexts[0]);
        let poison = thread::spawn(move || {
            let _control = context.control.lock().unwrap();
            panic!("poison control");
        });
        assert!(poison.join().is_err());
        let mut workers = Workers {
            tile,
            threads: Vec::new(),
            reported: AtomicBool::new(false),
        };
        assert!(workers.shutdown().unwrap_err().contains("control mutex poisoned"));
    }
}
