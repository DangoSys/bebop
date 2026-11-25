use super::execute::Accelerator;
use generic::trace;
use generic::{configs, stores::Stores};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub struct ThreadModel {
    pub(crate) stores: Arc<Mutex<Stores>>,
    pub(crate) acc: Accelerator,
}

impl ThreadModel {
    /// The model of the thread behind the task port `context`; it selects the thread's
    /// configuration.
    pub fn new(
        context: &generic::tasks::Context,
        log_dir: &Path,
        trace: trace::TraceConfig,
        profile: bool,
        balls: generic::blink::BallBus,
    ) -> Result<Self, String> {
        configs::configure_thread(context.thread);
        std::fs::create_dir_all(log_dir).map_err(|error| error.to_string())?;
        let endpoint = generic::configs::Endpoint::Thread(context.thread);
        Ok(Self {
            stores: Arc::clone(&context.stores),
            acc: Accelerator::new(log_dir, trace, profile, endpoint.execution_id(), balls)?,
        })
    }
}
