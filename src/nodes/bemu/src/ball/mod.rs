mod registry;

pub use crate::thread::instructions::context::ExecContext;
pub use linkme;
pub(crate) use registry::{cycles_after_issue, execute_known};
pub use registry::{Ball, BallModel, BallStates, BALLS};
