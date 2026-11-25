//! The device level of BEMU: the peripherals and the link a device offers its harts.
pub mod platform;
pub mod plic;

mod model;

pub use model::DeviceModel;
