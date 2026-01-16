use bebop::simulator::config::config::AppConfig;
use bebop::simulator::utils::log::init_log;
use bebop::simulator::Simulator;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

// Global mutex to ensure only one test runs at a time (avoid port conflicts)
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

fn get_app_config(test_binary_name: &str) -> AppConfig {
  AppConfig {
    host: bebop::simulator::config::config::HostSection {
      host_type: "spike".to_string(),
      spike: Some(bebop::simulator::config::config::HostTypeConfig {
        host_path: get_host_path(),
        test_binary_path: get_workspace_root()
          .join(format!(
            "bb-tests/output/workloads/src/OpTest/gemmini/{}",
            test_binary_name
          ))
          .to_string_lossy()
          .to_string(),
        host_args: vec!["--extension=bebop".to_string()],
        gem5_mode: String::new(),
        se_binary_path: String::new(),
        fs_kernel_path: String::new(),
        fs_image_path: String::new(),
      }),
      gem5: None,
    },
    simulation: bebop::simulator::config::config::SimulationSection {
      arch_type: "gemmini".to_string(),
      quiet: false,
      step_mode: false,
      trace_file: String::new(),
    },
  }
}

macro_rules! test_case {
  ($name:ident, $binary:literal) => {
    #[test]
    #[cfg(feature = "bb-tests")]
    fn $name() {
      // Acquire mutex to ensure only one test runs at a time
      let _guard = TEST_MUTEX.lock().unwrap();
      init_log();

      let app_config = get_app_config($binary);
      let mut simulator = Simulator::from_app_config(&app_config).expect("Failed to create simulator");
      simulator.run().expect("Simulator run failed");

      // Wait for port release (TIME_WAIT state usually takes a few seconds)
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
