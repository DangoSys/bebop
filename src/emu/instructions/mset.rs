use super::super::config::{BankConfig, BANK_NUM};
/// MSET 指令实现 (funct=23)
/// 功能：设置矩阵参数（分配/释放 bank）
///
/// 宏定义：bb_mset(bank_id, alloc, row, col)
/// xs1 = BB_BANK0(bank_id) | BB_WR
/// xs2 = FIELD(row, 0, 4) | FIELD(col, 5, 9) | FIELD(alloc, 10, 10)
use log::{error, info};

/// MSET 指令执行
///
/// # Arguments
/// * `xs1` - 包含 bank_id
/// * `xs2` - 包含 row, col, alloc
/// * `bank_configs` - Bank 配置数组
/// * `banks` - Bank 内存数组
pub fn execute_mset(
    xs1: u64,
    xs2: u64,
    bank_configs: &mut [BankConfig],
    banks: &mut [Vec<u8>],
) -> u64 {
    // 解码 xs1：提取 bank_id (低 5 位)
    let bank_id = xs1 & 0x1F;

    // 解码 xs2：按照宏定义提取 row, col, alloc
    let row = xs2 & 0x1F; // bits 0-4
    let col = (xs2 >> 5) & 0x1F; // bits 5-9
    let alloc = (xs2 >> 10) & 0x1; // bit 10

    info!(
        "MSET: bank_id={}, alloc={}, row={}, col={}",
        bank_id, alloc, row, col
    );

    if bank_id < BANK_NUM as u64 {
        // 检查重复分配
        if alloc == 1 && bank_configs[bank_id as usize].allocated {
            error!("MSET: Bank {} already allocated", bank_id);
            return 0;
        }

        bank_configs[bank_id as usize] = BankConfig {
            allocated: alloc == 1,
            rows: row,
            cols: col,
            bank_id,
        };

        if alloc == 1 {
            info!("MSET: Allocated bank {} ({}x{})", bank_id, row, col);
            // 清零 bank 内存
            banks[bank_id as usize].fill(0);
        } else {
            info!("MSET: Released bank {}", bank_id);
        }
    } else {
        error!("MSET: Invalid bank_id={}", bank_id);
    }

    0
}
