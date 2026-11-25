//! Thread-level parallelism in BEMU: the hardware threads of a hart (arch: behind its TBus),
//! each executing on a host thread of its own, the timing unit that keeps their time, and the
//! task protocol they serve.
mod group;
mod unit;

pub use group::ThreadGroup;
