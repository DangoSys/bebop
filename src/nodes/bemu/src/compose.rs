use bebop_bemu_generic::configs::{types::CpuPipeline, CoreParams};
use bebop_bemu_generic::timing::CpuTiming;
use bebop_bemu_ilp::{CachedRocket, Rocket};
use bebop_bemu_mlp::DirectMapped;

/// A CPU's timing as its configuration composes it: the base sequential CPU, or a Rocket pipeline
/// (ilp) with its L1 caches (mlp).
pub fn cpu_timing(params: &CoreParams) -> Box<dyn CpuTiming> {
    match params.pipeline {
        CpuPipeline::Sequential => Box::new(bebop_bemu_hart::Sequential::new(params.fetch_line)),
        // Rocket's L1 I$ and D$: 64 direct-mapped 64-byte lines, virtually tagged; a miss refills
        // its line through the hart's coherent caches.
        CpuPipeline::Rocket => Box::new(Rocket::new(
            Box::new(DirectMapped::new(64, 40)),
            Box::new(DirectMapped::new(64, 40)),
        )),
        // CachedRocket's I$: 16 KiB of 64-byte lines; a miss refills a line from the L2.
        CpuPipeline::CachedRocket => Box::new(CachedRocket::new(Box::new(DirectMapped::new(256, 25)))),
    }
}
