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
      "bb-tests/output/workloads/src/OpTest/gemmini/{}",
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
// test_case!(conv_2d_nchw_fchw_f32, "conv_2d_nchw_fchw_f32-baremetal");
// test_case!(conv_2d_nchw_fchw_i8, "conv_2d_nchw_fchw_i8-baremetal");
// test_case!(conv_2d_nhwc_fhwc_f32, "conv_2d_nhwc_fhwc_f32-baremetal");
// test_case!(conv_2d_nhwc_hwcf_5x5_i8, "conv_2d_nhwc_hwcf_5x5_i8-baremetal");
// test_case!(tile_conv_igelu, "tile-conv-igelu-baremetal");
// test_case!(tile_conv_layernorm, "tile-conv-layernorm-baremetal");
// test_case!(tile_conv_relu, "tile-conv-relu-baremetal");
// test_case!(tile_conv_softmax, "tile-conv-softmax-baremetal");
// test_case!(tile_conv_base, "tile-conv-baremetal");
// test_case!(conv_2d_nhwc_fhwc_5x5_i8, "conv_2d_nhwc_fhwc_5x5_i8-baremetal");
// test_case!(conv_2d_nhwc_fhwc_i8, "conv_2d_nhwc_fhwc_i8-baremetal");
// test_case!(conv_2d_nhwc_hwcf_f32, "conv_2d_nhwc_hwcf_f32-baremetal");
// test_case!(conv_2d_nhwc_hwcf_i8, "conv_2d_nhwc_hwcf_i8-baremetal");

// ---------------------------------
// test passed
// ---------------------------------
test_case!(batch_matmul, "batch_matmul-baremetal");
test_case!(compute_accumulated, "compute-accumulated-baremetal");
test_case!(matmul_base, "matmul-baremetal");
test_case!(matmul_os_base, "matmul-os-baremetal");
test_case!(matmul_ws_base, "matmul-ws-baremetal");
test_case!(matrix_add, "matrix-add-baremetal");
test_case!(matrix_add_scale, "matrix-add-scale-baremetal");
test_case!(mvin_mvout, "mvin-mvout-baremetal");
test_case!(tile_matmul_base, "tile-matmul-baremetal");
test_case!(tile_matmul_os, "tile-matmul-os-baremetal");
test_case!(tile_matmul_ws_igelu, "tile-matmul-ws-igelu-baremetal");
test_case!(tile_matmul_ws_layernorm, "tile-matmul-ws-layernorm-baremetal");
test_case!(tile_matmul_ws_relu, "tile-matmul-ws-relu-baremetal");
test_case!(tile_matmul_ws_softmax, "tile-matmul-ws-softmax-baremetal");
test_case!(tile_rect_conv, "tile-rect-conv-baremetal");
test_case!(transpose, "transpose-baremetal");
