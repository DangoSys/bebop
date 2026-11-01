pub(crate) mod ant;
pub mod bank_matrix;
pub mod banks;
pub(crate) mod execute;
pub mod instructions;
pub mod kernels;
pub mod trace;

mod model;
mod timing;

pub use execute::Accelerator;
pub use model::ThreadModel;
