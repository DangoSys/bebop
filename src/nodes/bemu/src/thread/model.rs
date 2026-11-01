use super::{ant, execute::Accelerator, trace};
use crate::{configs, core::CoreState};
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
        let mut acc = Accelerator::new(log_dir, trace, profile, endpoint.execution_id(), None, None)?;
        acc.task = Some(Arc::clone(context));
        Ok(Self {
            ant: Arc::clone(&context.execution),
            acc,
        })
    }
}
