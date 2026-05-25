//===- instruction_register.rs - Instruction registry ----------------------===//

use super::instruction::Instruction;

// Instruction registration - add new instructions here
macro_rules! register_instructions {
    ($($module:ident :: $name:ident),* $(,)?) => {
        pub fn execute_known(
            funct: u32,
            xs1: u64,
            xs2: u64,
            ctx: &mut super::instruction::ExecContext,
        ) -> Option<u64> {
            match funct {
                $(
                    <super::$module::$name as Instruction>::FUNCT => {
                        Some(<super::$module::$name as Instruction>::exec(xs1, xs2, ctx))
                    }
                )*
                _ => None,
            }
        }

        pub fn cycles_after_issue(funct: u32, xs1: u64, xs2: u64) -> u64 {
            match funct {
                $(
                    <super::$module::$name as Instruction>::FUNCT => {
                        <super::$module::$name as Instruction>::latency(xs1, xs2)
                    }
                )*
                _ => 1,
            }
        }
    };
}

register_instructions! {
    f00_fence::Fence,
    f01_barrier::Barrier,
    f02_gemmini_config::GemminiConfig,
    f03_gemmini_flush::GemminiFlush,
    f04_bdb_counter::BdbCounter,
    f16_mvout::Mvout,
    f32_mset::Mset,
    f33_mvin::Mvin,
    f34_mmio_set::MmioSet,
    f35_mvin_mmio::MvinMmio,
    f48_im2col::Im2col,
    f49_transpose::Transpose,
    f50_relu::Relu,
    f51_fp2int::Fp2Int,
    f52_int2fp::Int2Fp,
    f53_gemmini_preload::GemminiPreload,
    f55_mxfp2int::Mxfp2Int,
    f64_mul_warp16::MulWarp16,
    f65_bfp::Bfp,
    f66_gemmini_compute_preloaded::GemminiComputePreloaded,
    f67_gemmini_compute_accumulated::GemminiComputeAccumulated,
    f80_gemmini_loop_ws::GemminiLoopWsConfigBounds,
    f80_gemmini_loop_ws::GemminiLoopWsConfigAddrA,
    f80_gemmini_loop_ws::GemminiLoopWsConfigAddrB,
    f80_gemmini_loop_ws::GemminiLoopWsConfigAddrD,
    f80_gemmini_loop_ws::GemminiLoopWsConfigAddrC,
    f80_gemmini_loop_ws::GemminiLoopWsConfigStridesAB,
    f80_gemmini_loop_ws::GemminiLoopWsConfigStridesDC,
    f80_gemmini_loop_ws::GemminiLoopWs,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig1,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig2,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig3,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig4,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig5,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig6,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig7,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig8,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWsConfig9,
    f96_gemmini_loop_conv_ws::GemminiLoopConvWs,
}
