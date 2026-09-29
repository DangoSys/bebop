use rvsim::{csr::Csrs, Privilege};
use std::sync::{Condvar, Mutex};

pub(crate) struct Task {
    pub entry: u64,
    pub argument: u64,
    pub stack: u64,
    pub tls: u64,
    pub gp: u64,
    pub workspace: u64,
    pub completion: u64,
    pub csrs: Csrs,
    pub privilege: Privilege,
}

struct Slot {
    pending: Option<Task>,
    active: bool,
    result: u64,
    workspace: u64,
}

struct State {
    slots: Vec<Slot>,
    controller_workspace: u64,
    stopped: bool,
}

pub(crate) struct Tasks {
    pub signatures: Vec<u64>,
    state: Mutex<State>,
    ready: Condvar,
}

impl Tasks {
    pub fn new(signatures: Vec<u64>) -> Self {
        Self {
            state: Mutex::new(State {
                slots: (0..signatures.len())
                    .map(|_| Slot {
                        pending: None,
                        active: false,
                        result: 1,
                        workspace: 0,
                    })
                    .collect(),
                controller_workspace: 0,
                stopped: false,
            }),
            signatures,
            ready: Condvar::new(),
        }
    }

    pub fn submit(&self, core: usize, signature: u64, task: Task) -> Result<(), String> {
        if self.signatures.get(core) != Some(&signature) {
            return Err(format!("task requires an incompatible core signature: core {core}"));
        }
        let mut state = self.state.lock().expect("tile task state poisoned");
        let slot = &mut state.slots[core];
        if slot.active {
            return Err(format!("core {core} already has a task"));
        }
        slot.workspace = task.workspace;
        slot.pending = Some(task);
        slot.active = true;
        slot.result = 0;
        self.ready.notify_all();
        Ok(())
    }

    pub fn take(&self, core: usize) -> Option<Task> {
        let mut state = self.state.lock().expect("tile task state poisoned");
        loop {
            if state.stopped {
                return None;
            }
            if let Some(task) = state.slots[core].pending.take() {
                return Some(task);
            }
            state = self.ready.wait(state).expect("tile task state poisoned");
        }
    }

    pub fn complete(&self, core: usize, status: u64) {
        let mut state = self.state.lock().expect("tile task state poisoned");
        let slot = &mut state.slots[core];
        assert!(slot.active, "completion without a running task");
        slot.active = false;
        slot.result = if status == 0 { 1 } else { 2 };
        self.ready.notify_all();
    }

    pub fn wait(&self, core: usize) -> u64 {
        let mut state = self.state.lock().expect("tile task state poisoned");
        while state.slots[core].active && !state.stopped {
            state = self.ready.wait(state).expect("tile task state poisoned");
        }
        state.slots[core].result
    }

    pub fn set_workspace(&self, address: u64) {
        self.state
            .lock()
            .expect("tile task state poisoned")
            .controller_workspace = address;
    }

    pub fn workspace(&self, hart: usize) -> u64 {
        let state = self.state.lock().expect("tile task state poisoned");
        if hart == 0 {
            state.controller_workspace
        } else {
            state.slots[hart - 1].workspace
        }
    }

    pub fn stop(&self) {
        self.state.lock().expect("tile task state poisoned").stopped = true;
        self.ready.notify_all();
    }

    pub fn stopped(&self) -> bool {
        self.state.lock().expect("tile task state poisoned").stopped
    }
}
