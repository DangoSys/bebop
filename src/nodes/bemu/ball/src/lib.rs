//! The Ball level of BEMU: the registry Ball models join, the Ball bus, and the boundary a
//! Ball model sees (`blink`).
pub mod blink;
mod registry;

pub use generic::blink::ExecContext;
pub use linkme;
pub use registry::{Ball, BallModel, BALLS, BUS};
