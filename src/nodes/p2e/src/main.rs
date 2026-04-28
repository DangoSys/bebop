use bebop_p2e::{P2ESimulator, ScuController};
use std::fs;

fn main() -> Result<(), String> {
    env_logger::init();

    println!("P2E Basic Simulation Example");

    // 创建仿真器
    let sim = P2ESimulator::new()?;

    // 1. 加载 kernel image 到 DDR
    let kernel_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "kernel.bin".to_string());

    if let Ok(kernel) = fs::read(&kernel_path) {
        println!("Loading kernel from {} ({} bytes)", kernel_path, kernel.len());
        sim.load_image(0x80000000, &kernel)?;
    } else {
        println!("No kernel file found, skipping DDR load");
    }

    // 2. 复位仿真器
    println!("Resetting simulator...");
    sim.reset()?;

    // 3. 通过 UART 发送消息
    println!("Sending UART message...");
    ScuController::uart_puts("Hello from P2E!\n")?;

    // 4. 运行仿真
    println!("Running simulation...");
    sim.step(1000)?;

    // 5. 检查退出状态
    if sim.check_exit() {
        let code = sim.get_exit_code();
        println!("Simulation exited with code: {}", code);
    } else {
        println!("Simulation still running");
    }

    Ok(())
}
