use bebop::simulator::host::HostConfig;
use bebop::simulator::sim::mode::{ArchType, SimConfig, StepMode};
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
    arch_type: ArchType::Buckyball,
  }
}

fn set_binary_path(test_binary_name: &str) -> String {
  get_workspace_root()
    .join(format!(
      "bb-tests/output/workloads/src/CTest/bebop/{}",
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


// ---------------------------------
// test passed
// ---------------------------------
test_case!(ctest_mvin_mvout_bebop_test, "ctest_mvin_mvout_bebop_test_singlecore-baremetal");
