pub(crate) mod ant;
pub mod bank_matrix;
pub mod banks;
pub(crate) mod execute;
pub mod instructions;
pub mod kernels;
pub mod trace;

use crate::{configs, core::CoreState};
pub use execute::Accelerator;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub struct ThreadModel {
    pub(crate) ant: Arc<Mutex<ant::Ant>>,
    pub(crate) acc: Accelerator,
}

impl ThreadModel {
    pub fn new(
        core: &Arc<CoreState>,
        index: usize,
        log_dir: &Path,
        trace: trace::TraceConfig,
        profile: bool,
    ) -> Result<Self, String> {
        configs::configure_thread(index);
        std::fs::create_dir_all(log_dir).map_err(|error| error.to_string())?;
        let context = core
            .tasks
            .contexts
            .iter()
            .find(|context| context.thread == index)
            .expect("thread has no task port");
        let endpoint = crate::Endpoint::Thread(index);
        let acc = Accelerator::new(
            log_dir,
            trace,
            profile,
            endpoint.execution_id(),
            Some(core.endpoint_index(endpoint)),
            Some(Arc::clone(core)),
        )?;
        Ok(Self {
            ant: Arc::clone(&context.execution),
            acc,
        })
    }
}
