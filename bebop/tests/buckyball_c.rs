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
            "bb-tests/output/workloads/src/CTest/bebop/{}",
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
      arch_type: "buckyball".to_string(),
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

// ---------------------------------
// test passed
// ---------------------------------
test_case!(
  ctest_mvin_mvout_bebop_test,
  "ctest_mvin_mvout_bebop_test_singlecore-baremetal"
);
