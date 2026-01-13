use bebop::simulator::host::host::HostConfig;
use bebop::simulator::sim::mode::{ArchType, HostType, SimConfig, StepMode};
use bebop::simulator::utils::log::init_log;
use bebop::simulator::Simulator;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

// 全局互斥锁，确保同一时间只有一个测试运行（避免端口冲突）
static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn get_workspace_root() -> PathBuf {
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  manifest_dir.parent().unwrap().parent().unwrap().to_path_buf()
}

fn get_host_path() -> String {
  get_workspace_root()
    .join("bebop/host/spike/riscv-isa-sim/install/bin/spike")
    .to_string_lossy()
    .to_string()
}

fn get_sim_config() -> SimConfig {
  SimConfig {
    quiet: false,
    step_mode: StepMode::Continuous,
    trace_file: None,
    arch_type: ArchType::Gemmini,
    host_type: HostType::Spike,
    host_config: None,
  }
}

fn set_binary_path(test_binary_name: &str) -> String {
  get_workspace_root()
    .join(format!(
      "bb-tests/output/workloads/src/CTest/gemmini/{}",
      test_binary_name
    ))
    .to_string_lossy()
    .to_string()
}

fn set_host_config(test_binary_name: &str) -> HostConfig {
  HostConfig {
    host: get_host_path(),
    arg: vec!["--extension=bebop".to_string(), set_binary_path(test_binary_name)],
  }
}

macro_rules! test_case {
  ($name:ident, $binary:literal) => {
    #[test]
    #[cfg(feature = "bb-tests")]
    fn $name() {
      // 获取互斥锁，确保同一时间只有一个测试运行
      let _guard = TEST_MUTEX.lock().unwrap();
      init_log();

      let host_config = set_host_config($binary);
      let mut simulator = Simulator::new(get_sim_config(), host_config).expect("Failed to create simulator");
      simulator.run().expect("Simulator run failed");

      // 等待端口释放（TIME_WAIT 状态通常需要几秒钟）
      drop(simulator);
      thread::sleep(Duration::from_millis(500));
    }
  };
}

// ---------------------------------
// test failed
// ---------------------------------
// test_case!(test_gemmini_conv_rect, "gemmini_conv_rect_singlecore-baremetal");
// test_case!(test_gemmini_conv_base, "gemmini_conv_singlecore-baremetal");
// test_case!(test_gemmini_conv_stride, "gemmini_conv_stride_singlecore-baremetal");
// test_case!(
//   test_gemmini_conv_trans_input_3120,
//   "gemmini_conv_trans_input_3120_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_trans_input_3120_with_kernel_dilation,
//   "gemmini_conv_trans_input_3120_with_kernel_dilation_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_trans_output_1203,
//   "gemmini_conv_trans_output_1203_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_trans_weight_0132,
//   "gemmini_conv_trans_weight_0132_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_trans_weight_1203,
//   "gemmini_conv_trans_weight_1203_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_with_input_dilation_and_neg_padding,
//   "gemmini_conv_with_input_dilation_and_neg_padding_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_with_input_dilation_and_rot180,
//   "gemmini_conv_with_input_dilation_and_rot180_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_with_input_dilation,
//   "gemmini_conv_with_input_dilation_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_with_kernel_dilation,
//   "gemmini_conv_with_kernel_dilation_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_with_pool,
//   "gemmini_conv_with_pool_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_with_rot180,
//   "gemmini_conv_with_rot180_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_gemmini_counter,
//   "gemmini_gemmini_counter_singlecore-baremetal"
// );

// test_case!(
//   test_gemmini_mvin_mvout_acc_full,
//   "gemmini_mvin_mvout_acc_full_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_mvin_mvout_acc_full_stride,
//   "gemmini_mvin_mvout_acc_full_stride_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_mvin_mvout_acc,
//   "gemmini_mvin_mvout_acc_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_mvin_mvout_acc_stride,
//   "gemmini_mvin_mvout_acc_stride_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_mvin_mvout_acc_zero_stride,
//   "gemmini_mvin_mvout_acc_zero_stride_singlecore-baremetal"
// );

// test_case!(
//   test_gemmini_tiled_matmul_option,
//   "gemmini_tiled_matmul_option_singlecore-baremetal"
// );

// test_case!(
//   test_gemmini_tiled_matmul_ws_igelu,
//   "gemmini_tiled_matmul_ws_igelu_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_tiled_matmul_ws_layernorm,
//   "gemmini_tiled_matmul_ws_layernorm_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_tiled_matmul_ws_softmax,
//   "gemmini_tiled_matmul_ws_softmax_singlecore-baremetal"
// );
// test_case!(
//   test_gemmini_conv_first_layer,
//   "gemmini_conv_first_layer_singlecore-baremetal"
// );

// ---------------------------------
// test passed
// ---------------------------------
test_case!(test_gemmini_conv_dw_base, "gemmini_conv_dw_singlecore-baremetal");
test_case!(test_gemmini_aligned, "gemmini_aligned_singlecore-baremetal");
test_case!(test_gemmini_transpose, "gemmini_transpose_singlecore-baremetal");
test_case!(
  test_gemmini_tiled_matmul_ws_base,
  "gemmini_tiled_matmul_ws_singlecore-baremetal"
);
test_case!(
  test_gemmini_tiled_matmul_ws_low_D,
  "gemmini_tiled_matmul_ws_low_D_singlecore-baremetal"
);
test_case!(
  test_gemmini_tiled_matmul_ws_perf,
  "gemmini_tiled_matmul_ws_perf_singlecore-baremetal"
);
test_case!(
  test_gemmini_mvin_mvout_zeros,
  "gemmini_mvin_mvout_zeros_singlecore-baremetal"
);
test_case!(
  test_gemmini_tiled_matmul_cpu,
  "gemmini_tiled_matmul_cpu_singlecore-baremetal"
);
test_case!(test_gemmini_mvin_scale, "gemmini_mvin_scale_singlecore-baremetal");
test_case!(test_gemmini_padded, "gemmini_padded_singlecore-baremetal");
test_case!(test_gemmini_raw_hazard, "gemmini_raw_hazard_singlecore-baremetal");
test_case!(test_gemmini_resadd_base, "gemmini_resadd_singlecore-baremetal");
test_case!(test_gemmini_resadd_stride, "gemmini_resadd_stride_singlecore-baremetal");
test_case!(test_gemmini_template, "gemmini_template_singlecore-baremetal");
test_case!(
  test_gemmini_tiled_matmul_ws_full_C,
  "gemmini_tiled_matmul_ws_full_C_singlecore-baremetal"
);
test_case!(
  test_gemmini_tiled_matmul_ws_At,
  "gemmini_tiled_matmul_ws_At_singlecore-baremetal"
);
test_case!(
  test_gemmini_tiled_matmul_ws_Bt,
  "gemmini_tiled_matmul_ws_Bt_singlecore-baremetal"
);
test_case!(
  test_gemmini_tiled_matmul_os,
  "gemmini_tiled_matmul_os_singlecore-baremetal"
);
test_case!(test_gemmini_mvin_mvout, "gemmini_mvin_mvout_singlecore-baremetal");
test_case!(
  test_gemmini_mvin_mvout_stride,
  "gemmini_mvin_mvout_stride_singlecore-baremetal"
);
test_case!(
  test_gemmini_mvin_mvout_block_stride,
  "gemmini_mvin_mvout_block_stride_singlecore-baremetal"
);
test_case!(
  test_gemmini_global_average,
  "gemmini_global_average_singlecore-baremetal"
);
test_case!(test_gemmini_matmul_os, "gemmini_matmul_os_singlecore-baremetal");
test_case!(test_gemmini_matmul_base, "gemmini_matmul_singlecore-baremetal");
test_case!(test_gemmini_matmul_ws, "gemmini_matmul_ws_singlecore-baremetal");
test_case!(test_gemmini_matrix_add, "gemmini_matrix_add_singlecore-baremetal");
test_case!(test_gemmini_conv_dw_perf, "gemmini_conv_dw_perf_singlecore-baremetal");
test_case!(test_gemmini_conv_perf, "gemmini_conv_perf_singlecore-baremetal");
test_case!(
  test_gemmini_conv_rect_pool,
  "gemmini_conv_rect_pool_singlecore-baremetal"
);
