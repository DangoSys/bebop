//===- dispatch.rs - Base BEMU instruction set --------------------------------===//

use super::context::{ExecContext, Instruction};

macro_rules! register_instructions {
    ($($inst:path),* $(,)?) => {
        pub fn execute_known(
            funct: u32,
            xs1: u64,
            xs2: u64,
            ctx: &mut ExecContext,
        ) -> Option<u64> {
            assert_ne!(funct, 0, "funct7 zero is not a Buckyball instruction");
            match funct {
                $(
                    <$inst as Instruction>::FUNCT => {
                        Some(<$inst as Instruction>::exec(xs1, xs2, ctx))
                    }
                )*
                _ => {
                    let ball_class = crate::configs::ball_domain::ball_class_for_funct(funct)
                        .unwrap_or_else(|| panic!("funct7 {funct} is not declared in BEMU ballISA TOML"));
                    Some(crate::ball::execute_known(&ball_class, funct, xs1, xs2, ctx))
                },
            }
        }

        pub fn cycles_after_issue(funct: u32, xs1: u64, xs2: u64) -> u64 {
            assert_ne!(funct, 0, "funct7 zero is not a Buckyball instruction");
            match funct {
                // Source read plus target write; mesh transport is modeled separately.
                13 => (((xs2 >> 32) & 0xffff) + 1) * 2,
                $(
                    <$inst as Instruction>::FUNCT => {
                        <$inst as Instruction>::latency(xs1, xs2)
                    }
                )*
                _ => {
                    let ball_class = crate::configs::ball_domain::ball_class_for_funct(funct)
                        .unwrap_or_else(|| panic!("funct7 {funct} is not declared in BEMU ballISA TOML"));
                    crate::ball::cycles_after_issue(&ball_class, funct, xs1, xs2)
                },
            }
        }

    };
}

register_instructions! {
    super::barrier::Barrier,
    crate::thread::instructions::mvout::Mvout,
    crate::thread::instructions::kernel::MvinKernel,
    crate::thread::instructions::kernel::RunKernel,
    crate::thread::instructions::mset::Mset,
    crate::thread::instructions::mvin::Mvin,
    crate::thread::instructions::mvin_2d::Mvin2d,
    crate::thread::instructions::mvin_mmio::MvinMmio,
}
