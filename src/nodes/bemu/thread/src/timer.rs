use crate::timing::Timing;
use generic::tasks::{Event, Record, ThreadTimer};
use generic::tlink::TLink;

/// The time of a hardware thread: the cycle of its Ant CPU and the event times of its
/// accelerator. The thread's execution runs ahead without it; the timing unit of the hart
/// replays the execution's records into it.
pub(crate) struct Timer {
    cycle: u64,
    timing: Timing,
}

impl Timer {
    /// The timer of the thread whose configuration is selected.
    pub(crate) fn new() -> Self {
        Self {
            cycle: 0,
            timing: generic::configs::with_accelerator(|p| {
                Timing::new(
                    p.ball_domain
                        .balls
                        .iter()
                        .map(|ball| ball.id as usize + 1)
                        .max()
                        .unwrap_or(0),
                    p.rob_entries,
                )
            }),
        }
    }
}

impl ThreadTimer for Timer {
    fn accepted(&self, record: &Record) -> Option<u64> {
        match &record.event {
            // A DMA with no DDR range stays in hart-local storage and reserves nothing.
            Event::Custom {
                transfer: Some(transfer),
                ..
            } if !transfer.ranges.is_empty() => Some(self.timing.ready(self.cycle + record.ant)),
            Event::Shared => Some(self.cycle + record.ant),
            _ => None,
        }
    }

    fn replay(&mut self, link: &TLink, record: &Record) -> (u64, u64) {
        self.cycle += record.ant;
        match &record.event {
            Event::Begin { cycle } => {
                self.cycle = *cycle;
                self.timing.reset();
            }
            // Executor + LocalStore take four clocks; the fence then waits on the
            // accelerator's event timeline.
            Event::Fence => self.cycle = (self.cycle + 4).max(self.timing.complete),
            Event::Custom {
                funct7,
                xs1,
                xs2,
                ball,
                cycles,
                transfer,
            } => {
                let issued = self.timing.issue(self.cycle, *funct7, *xs1, *xs2, *ball, |start| {
                    cycles
                        + transfer.as_ref().map_or(0, |transfer| {
                            transfer.cycles(&mut link.transfers.lock().unwrap(), start)
                        })
                });
                self.cycle = self.cycle.max(self.timing.accepted);
                return issued;
            }
            Event::End { .. } => self.cycle = self.cycle.max(self.timing.complete),
            Event::Run | Event::Shared | Event::Cycle | Event::Fault(_) => {}
        }
        (self.cycle, 0)
    }

    fn resume(&mut self, cycle: u64) {
        self.cycle = self.cycle.max(cycle);
    }

    fn cycles(&self) -> (u64, u64) {
        (self.cycle, self.timing.complete)
    }
}
