use super::timer::Timer;
use super::{ant::Ant, ThreadModel};
use generic::riscv::registers::Registers;
use generic::tasks::{Context, Event, Record, ThreadTimer, ThreadWorker};
use generic::tlink::TLink;
use std::sync::Arc;

impl ThreadWorker for ThreadModel {
    fn run(
        &mut self,
        link: &Arc<TLink>,
        index: usize,
        context: &Context,
        descriptor: [u64; 6],
        start_cycle: u64,
        dma: &(generic::riscv::csr::Csrs, generic::riscv::Privilege),
    ) {
        let tasks = &link.tasks;
        let mut stores = self.stores.lock().unwrap();
        let mut execution = Ant {
            cpu: Registers::new(0, descriptor[1]),
        };
        execution.cpu.set_register(2, descriptor[4]);
        execution.cpu.set_register(4, stores.tls.base);
        execution.cpu.set_register(10, descriptor[3]);
        tasks.record(
            index,
            Record {
                ant: 0,
                event: Event::Begin { cycle: start_cycle },
            },
        );
        self.acc.trace.task_event("begin", descriptor[0], start_cycle);
        let Some((ant, value)) =
            execution.execute(&mut stores, link, index, context, &mut self.acc, descriptor[2], dma)
        else {
            return;
        };
        // The task is done for the hart once the timing unit has replayed its end.
        let record = Record {
            ant,
            event: Event::End { value },
        };
        let Some((cycle, _)) = tasks.sync(index, record) else {
            return;
        };
        self.acc.trace.task_event("end", descriptor[0], cycle);
    }

    fn finish(&mut self, thread: usize, elapsed: std::time::Duration) {
        self.acc.finish_bank_frees();
        if let Some(operator) = &self.acc.operator {
            operator.write_profile();
        }
        if let Some(report) = self.acc.profile.report(elapsed, std::time::Duration::ZERO) {
            eprintln!("BEMU Ant {thread}\n{}", bebop_bemu_profile::format_report(&report));
        }
    }
}

/// The hardware threads of a hart, one per thread of its link: the worker that executes its
/// tasks, logging under `ant-<index>` of the log directory, and the timer of its time.
pub fn workers(
    link: &Arc<TLink>,
    balls: generic::blink::BallBus,
    options: &generic::options::ModelOptions,
) -> Result<Vec<(Box<dyn ThreadWorker>, Box<dyn ThreadTimer>)>, String> {
    link.tasks
        .contexts
        .iter()
        .enumerate()
        .map(|(index, context)| {
            let dir = options.log_dir.join(format!("ant-{index}"));
            let model = ThreadModel::new(context, &dir, options.trace.clone(), options.profile, balls)?;
            Ok((
                Box::new(model) as Box<dyn ThreadWorker>,
                Box::new(Timer::new()) as Box<dyn ThreadTimer>,
            ))
        })
        .collect()
}
