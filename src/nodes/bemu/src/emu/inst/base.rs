//===- base.rs - Base BEMU instruction set --------------------------------===//

use super::instruction::{ExecContext, Instruction};

macro_rules! register_instructions {
    ($($inst:path),* $(,)?) => {
        pub fn execute_known(
            funct: u32,
            xs1: u64,
            xs2: u64,
            ctx: &mut ExecContext,
        ) -> Option<u64> {
            match funct {
                $(
                    <$inst as Instruction>::FUNCT => {
                        Some(<$inst as Instruction>::exec(xs1, xs2, ctx))
                    }
                )*
                _ => None,
            }
        }

        pub fn cycles_after_issue(funct: u32, xs1: u64, xs2: u64) -> u64 {
            match funct {
                $(
                    <$inst as Instruction>::FUNCT => {
                        <$inst as Instruction>::latency(xs1, xs2)
                    }
                )*
                _ => 1,
            }
        }
    };
}

register_instructions! {
    super::f00_fence::Fence,
    super::f01_barrier::Barrier,
    super::f16_mvout::Mvout,
    super::f32_mset::Mset,
    super::f33_mvin::Mvin,
    super::f34_mmio_set::MmioSet,
    super::f35_mvin_mmio::MvinMmio,
}
