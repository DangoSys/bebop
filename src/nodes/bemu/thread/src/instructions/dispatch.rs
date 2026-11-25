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
                    let ball_class = generic::configs::ball_domain::ball_class_for_funct(funct)
                        .unwrap_or_else(|| panic!("funct7 {funct} is not declared in BEMU ballISA TOML"));
                    Some((ctx.balls.execute)(&ball_class, funct, xs1, xs2, ctx))
                },
            }
        }

        pub fn cycles_after_issue(funct: u32, xs1: u64, xs2: u64, ctx: &ExecContext) -> u64 {
            assert_ne!(funct, 0, "funct7 zero is not a Buckyball instruction");
            match funct {
                $(
                    <$inst as Instruction>::FUNCT => {
                        <$inst as Instruction>::latency(xs1, xs2, ctx)
                    }
                )*
                _ => {
                    let ball_class = generic::configs::ball_domain::ball_class_for_funct(funct)
                        .unwrap_or_else(|| panic!("funct7 {funct} is not declared in BEMU ballISA TOML"));
                    (ctx.balls.cycles)(&ball_class, funct, xs1, xs2, ctx)
                },
            }
        }

        pub fn transfer_after_issue(
            funct: u32,
            xs1: u64,
            xs2: u64,
            ctx: &ExecContext,
        ) -> Option<generic::guest::Transfer> {
            match funct {
                $(
                    <$inst as Instruction>::FUNCT => {
                        <$inst as Instruction>::transfer(xs1, xs2, ctx)
                    }
                )*
                _ => None,
            }
        }
    };
}

register_instructions! {
    super::barrier::Barrier,
    super::dma_strided::MvinStrided,
    super::dma_strided::MvoutStrided,
    super::dma_strided::MvinBlocks,
    super::dma_strided::MvoutBlocks,
    crate::instructions::mvout::Mvout,
    crate::instructions::operator::MvinOperator,
    crate::instructions::operator::RunOperator,
    crate::instructions::mset::Mset,
    crate::instructions::mvin::Mvin,
    crate::instructions::mvin_2d::Mvin2d,
    crate::instructions::mvin_mmio::MvinMmio,
}
