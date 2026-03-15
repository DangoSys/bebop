/// Bemu 配置和数据类型定义
/// 包含内存配置常量、统计信息、Bank 配置等基础数据结构

/// 内存配置常量（与 Buckyball 硬件一致）
pub const BANK_NUM: usize = 32;        // 虚拟 bank 数量
pub const BANK_WIDTH: usize = 128;     // bank 宽度（位）
pub const BANK_LINES: usize = 1024;    // 每个 bank 的行数
pub const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8); // 16KB
pub const TOTAL_MEMORY_SIZE: usize = BANK_NUM * BANK_SIZE;  // 512KB

/// 矩阵大小（16x16 - WARP16）
pub const MATRIX_SIZE: usize = 16;

/// Bemu 统计信息
#[derive(Default, Clone, Copy, Debug)]
pub struct BemuStats {
    /// 执行的指令数
    pub instructions_executed: u64,
    /// 矩阵乘法执行次数
    pub matmul_count: u64,
    /// MSET 指令执行次数
    pub mset_count: u64,
    /// MVIN 指令执行次数
    pub mvin_count: u64,
    /// MVOUT 指令执行次数
    pub mvout_count: u64,
    /// TRANSPOSE 指令执行次数
    pub transpose_count: u64,
}

/// Bank 配置信息
#[derive(Default, Clone, Copy, Debug)]
pub struct BankConfig {
    /// 是否已分配
    pub allocated: bool,
    /// 行数（depth）
    pub rows: u64,
    /// 列数
    pub cols: u64,
    /// Bank ID
    pub bank_id: u64,
}
