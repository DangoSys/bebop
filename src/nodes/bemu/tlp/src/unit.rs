use generic::{
    tasks::{Event, ThreadTimer},
    tlink::TLink,
};
use std::sync::atomic::Ordering;

/// The timing unit of a hart, on a host thread of its own: it models the time of the hart's
/// threads and of the memory path they share. The threads execute ahead of their time and
/// report what they execute in records; the unit replays each thread's records into its
/// timer and publishes the thread's time, stalls and completion to the hart's CPU, which only
/// then sees them.
///
/// A record local to a thread, and anything of a cancelled thread, is replayed as it arrives.
/// A record that takes the hart's shared memory path is replayed in the order of (acceptance
/// cycle, thread): once the owner CPU has reached its acceptance, and no other thread can
/// still bring an earlier one. A thread with nothing queued brings its next record at its
/// published cycle or later (an idle thread has none); a stalled thread resumes at the cycle
/// its owner resolves it, which is the owner's cycle or later.
pub(crate) fn run(link: &TLink, mut timers: Vec<Box<dyn ThreadTimer>>) {
    let tasks = &link.tasks;
    // The threads whose task is stalled on a DMA page fault.
    let mut stalled = vec![false; timers.len()];
    loop {
        // What the choice below depends on is sampled before it: a change after this ends
        // the wait. The owner's clock comes before the state of its threads, which then
        // holds everything the owner did up to that cycle.
        let events = tasks.completion.events.load(Ordering::SeqCst);
        let clock = link.clint.shared_time(link.hart_id);
        if tasks.stopped.load(Ordering::Acquire) {
            return;
        }
        let mut local = None;
        let mut first: Option<(u64, usize)> = None;
        let mut bound = (u64::MAX, usize::MAX);
        let mut unresolved = None;
        for (index, context) in tasks.contexts.iter().enumerate() {
            let cancelled = context.cancelled.load(Ordering::Acquire);
            if stalled[index] {
                if context.resolved() {
                    local = Some(index);
                    break;
                }
                unresolved.get_or_insert(index);
                continue;
            }
            let records = context.records.lock().unwrap();
            let Some(record) = records.queue.front() else {
                if !cancelled {
                    bound = bound.min((context.progress.load(Ordering::Acquire), index));
                }
                continue;
            };
            let accepted = timers[index].accepted(record);
            drop(records);
            let Some(cycle) = accepted.filter(|_| !cancelled) else {
                local = Some(index);
                break;
            };
            // The owner CPU follows its threads to the acceptance they wait at.
            if context.progress.load(Ordering::Acquire) != cycle {
                context.advance(cycle, 0);
            }
            if first.is_none_or(|least| (cycle, index) < least) {
                first = Some((cycle, index));
            }
        }
        let (owner, follows) = tasks.owner_time(link.hart_id, clock);
        let index = match (local, first) {
            (Some(index), _) => index,
            (None, None) => {
                tasks.completion.sleep(events);
                continue;
            }
            (None, Some(accepted)) => {
                if let Some(index) = unresolved {
                    bound = bound.min((owner, index));
                }
                if owner >= accepted.0 && accepted < bound {
                    accepted.1
                } else if follows || (owner >= accepted.0 && unresolved.is_none()) {
                    // Only a record, or the owner leaving its wait, lets the unit go on.
                    tasks.completion.sleep(events);
                    continue;
                } else {
                    let running = link.clint.wait_shared(|| {
                        tasks.completion.events.load(Ordering::SeqCst) != events
                            || link.clint.shared_time(link.hart_id) != owner
                    });
                    if !running {
                        return;
                    }
                    continue;
                }
            }
        };
        let context = &tasks.contexts[index];
        let timer = &mut timers[index];
        if stalled[index] {
            if let Some(cycle) = context.resume() {
                timer.resume(cycle);
            }
            stalled[index] = false;
            context.block_shared(false);
            let (cycle, complete) = timer.cycles();
            context.advance(cycle, complete);
            context.replied((cycle, 0));
            continue;
        }
        let record = context.records.lock().unwrap().queue.pop_front().unwrap();
        let reply = timer.replay(link, &record);
        let (cycle, complete) = timer.cycles();
        match record.event {
            // The thread waits for the reply until its owner has resolved the fault.
            Event::Fault(trap) => {
                context.advance(cycle, complete);
                context.stall(trap);
                stalled[index] = true;
                continue;
            }
            Event::End { value } => context.finish(cycle, value),
            _ => context.advance(cycle, complete),
        }
        context.replied(reply);
    }
}
