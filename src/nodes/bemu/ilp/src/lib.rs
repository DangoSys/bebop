//! Instruction-level parallelism in BEMU: the pipelines that overlap a CPU's instructions.
mod cached_rocket;
mod rocket;

pub use cached_rocket::CachedRocket;
pub use rocket::Rocket;
