use rvsim::{csr::Csrs, Privilege};
use std::sync::{Condvar, Mutex};

pub(crate) struct Task {
    pub entry: u64,
    pub argument: u64,
    pub stack: u64,
    pub tls: u64,
    pub gp: u64,
    pub workspace: u64,
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
    descriptor: [u64; 7],
    descriptor_field: usize,
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
                descriptor: [0; 7],
                descriptor_field: 0,
                stopped: false,
            }),
            signatures,
            ready: Condvar::new(),
        }
    }

    pub fn stage(&self, field: usize, value: u64) -> Result<(), String> {
        let mut state = self.state.lock().expect("tile task state poisoned");
        if field != state.descriptor_field || field >= state.descriptor.len() {
            return Err("task descriptor fields must be staged in order".into());
        }
        state.descriptor[field] = value;
        state.descriptor_field += 1;
        Ok(())
    }

    pub fn descriptor(&self) -> Result<[u64; 7], String> {
        let mut state = self.state.lock().expect("tile task state poisoned");
        if state.descriptor_field != state.descriptor.len() {
            return Err("incomplete task descriptor".into());
        }
        state.descriptor_field = 0;
        Ok(state.descriptor)
    }

    pub fn poll(&self, core: usize) -> u64 {
        self.state.lock().expect("tile task state poisoned").slots[core].result
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

    pub fn has_pending(&self, core: usize) -> bool {
        self.state.lock().expect("tile task state poisoned").slots[core].pending.is_some()
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

    pub fn available(&self, signature: u64) -> bool {
        let state = self.state.lock().expect("tile task state poisoned");
        state.slots.iter().enumerate()
            .any(|(core, slot)| !slot.active && self.signatures[core] == signature)
    }

    pub fn wait_available(&self, signature: u64) -> u64 {
        let mut state = self.state.lock().expect("tile task state poisoned");
        while !state.stopped {
            if state
                .slots
                .iter()
                .enumerate()
                .any(|(core, slot)| !slot.active && self.signatures[core] == signature)
            {
                return 0;
            }
            state = self.ready.wait(state).expect("tile task state poisoned");
        }
        1
    }

    pub fn set_workspace(&self, address: u64) {
        self.state
            .lock()
            .expect("tile task state poisoned")
            .controller_workspace = address;
    }

    pub fn workspace(&self, worker: Option<usize>) -> u64 {
        let state = self.state.lock().expect("tile task state poisoned");
        match worker {
            Some(core) => state.slots[core].workspace,
            None => state.controller_workspace,
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
