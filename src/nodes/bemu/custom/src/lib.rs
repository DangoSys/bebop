//! Specialization in BEMU: the accelerator a hart carries on its RoCC port, the Balls plugged
//! directly into the hart, and its bank movement between the hart's endpoints.
pub(crate) mod execute;
pub mod instructions;

mod timing;
mod transfer;

pub use execute::Accelerator;
