/// Bemu - Bebop Emulator (软件模型/Golden Model)
/// 作用：
/// 1. 用软件快速实现自定义指令的语义
/// 2. 作为 Golden Model（参考模型）
/// 3. 与 RTL 实现对比（Difftest）
/// 根据 Buckyball C 代码宏定义实现指令语义：
/// - MSET (funct=23): 分配/释放内存 bank
/// - MVIN (funct=24): 从内存加载数据到 bank
/// - MVOUT (funct=25): 从 bank 存储数据到内存
/// - MUL_WARP16 (funct=32): 16x16 矩阵乘法
/// - TRANSPOSE (funct=34): 矩阵转置
use log::{debug, error, info};

/// 内存配置常量（与 Buckyball 硬件一致）
const BANK_NUM: usize = 32;        // 虚拟 bank 数量
const BANK_WIDTH: usize = 128;     // bank 宽度（位）
const BANK_LINES: usize = 1024;    // 每个 bank 的行数
const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8); // 16KB
const TOTAL_MEMORY_SIZE: usize = BANK_NUM * BANK_SIZE;  // 512KB

/// 矩阵大小（16x16 - WARP16）
const MATRIX_SIZE: usize = 16;

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

/// Bemu 模拟器状态
pub struct Bemu {
    /// 主内存空间
    memory: Vec<u8>,
    /// Bank 内存（每个 bank 16KB）
    banks: Vec<Vec<u8>>,
    /// 结果内存分配指针
    result_ptr: u64,
    /// 统计信息
    stats: BemuStats,
    /// Bank 配置
    bank_configs: [BankConfig; BANK_NUM],
}

impl Bemu {
    /// 创建新的 Bemu 实例
    pub fn new() -> Self {
        info!(
            "Creating Bemu (Bebop Emulator) - Golden Model\n  \
             Config: {} banks x {} bytes ({}KB total)",
            BANK_NUM, BANK_SIZE, (BANK_NUM * BANK_SIZE) / 1024
        );
        Self {
            memory: vec![0; TOTAL_MEMORY_SIZE],
            banks: (0..BANK_NUM).map(|_| vec![0; BANK_SIZE]).collect(),
            result_ptr: 0x1000,
            stats: BemuStats::default(),
            bank_configs: [BankConfig::default(); BANK_NUM],
        }
    }

    /// 执行自定义指令
    /// 
    /// # Arguments
    /// * `funct` - 指令功能码
    /// * `xs1` - 源操作数 1（根据指令编码不同参数）
    /// * `xs2` - 源操作数 2（根据指令编码不同参数）
    /// 
    /// # Returns
    /// * `u64` - 执行结果
    pub fn execute(&mut self, funct: u32, xs1: u64, xs2: u64) -> u64 {
        self.stats.instructions_executed += 1;
        
        debug!("Bemu executing: funct={}, xs1=0x{:x}, xs2=0x{:x}", funct, xs1, xs2);
        
        let result = match funct {
            // Buckyball 核心指令
            
            23 => self.execute_mset(xs1, xs2),          // MSET: 设置矩阵参数
            24 => self.execute_mvin(xs1, xs2),          // MVIN: 矩阵输入
            25 => self.execute_mvout(xs1, xs2),         // MVOUT: 矩阵输出
            32 => self.execute_mul_warp16(xs1, xs2),    // MUL_WARP16: 16x16 矩阵乘法
            34 => self.execute_transpose(xs1, xs2),     // TRANSPOSE: 矩阵转置
            
            _ => {
                error!("Bemu: Unknown funct={}", funct);
                0
            }
        };
        
        debug!("Bemu result: 0x{:x}", result);
        result
    }

    // ==================== MSET 指令 (funct=23) ====================
    
    /// MSET: 设置矩阵参数（分配/释放 bank）
    /// 
    /// 宏定义：bb_mset(bank_id, alloc, row, col)
    /// xs1 = BB_BANK0(bank_id) | BB_WR
    /// xs2 = FIELD(row, 0, 4) | FIELD(col, 5, 9) | FIELD(alloc, 10, 10)
    fn execute_mset(&mut self, xs1: u64, xs2: u64) -> u64 {
        self.stats.mset_count += 1;
        
        // 解码 xs1：提取 bank_id (低 5 位)
        let bank_id = xs1 & 0x1F;
        
        // 解码 xs2：按照宏定义提取 row, col, alloc
        let row = xs2 & 0x1F;           // bits 0-4
        let col = (xs2 >> 5) & 0x1F;    // bits 5-9
        let alloc = (xs2 >> 10) & 0x1;  // bit 10
        
        info!(
            "MSET: bank_id={}, alloc={}, row={}, col={}",
            bank_id, alloc, row, col
        );
        
        if bank_id < BANK_NUM as u64 {
            self.bank_configs[bank_id as usize] = BankConfig {
                allocated: alloc == 1,
                rows: row,
                cols: col,
                bank_id,
            };
            
            if alloc == 1 {
                info!("MSET: Allocated bank {} ({}x{})", bank_id, row, col);
                // 清零 bank 内存
                self.banks[bank_id as usize].fill(0);
            } else {
                info!("MSET: Released bank {}", bank_id);
            }
        } else {
            error!("MSET: Invalid bank_id={}", bank_id);
        }
        
        0
    }

    // ==================== MVIN 指令 (funct=24) ====================
    
    /// MVIN: 从内存加载数据到 bank
    /// 
    /// 宏定义：bb_mvin(mem_addr, bank_id, depth, stride)
    /// xs1 = BB_BANK0(bank_id) | BB_WR | FIELD(mem_addr, 27, 58)
    /// xs2 = FIELD(depth, 0, 9) | FIELD(stride, 10, 28)
    fn execute_mvin(&mut self, xs1: u64, xs2: u64) -> u64 {
        self.stats.mvin_count += 1;
        
        // 解码 xs1：提取 bank_id 和 mem_addr
        let bank_id = xs1 & 0x1F;              // bits 0-4
        let mem_addr = (xs1 >> 27) & 0xFFFFFFFF; // bits 27-58 (32 位地址)
        
        // 解码 xs2：提取 depth 和 stride
        let depth = xs2 & 0x3FF;              // bits 0-9 (10 bits)
        let stride = (xs2 >> 10) & 0x7FFFF;   // bits 10-28 (19 bits)
        
        info!(
            "MVIN: mem_addr=0x{:x}, bank_id={}, depth={}, stride={}",
            mem_addr, bank_id, depth, stride
        );
        
        if bank_id >= BANK_NUM as u64 {
            error!("MVIN: Invalid bank_id={}", bank_id);
            return 0;
        }
        
        if !self.bank_configs[bank_id as usize].allocated {
            error!("MVIN: Bank {} not allocated", bank_id);
            return 0;
        }
        
        // 从内存读取数据到 bank
        // stride 按 16 字节（128 位）为单位，与 Buckyball 硬件一致
        let actual_stride = if stride == 0 { 1 } else { stride };
        for i in 0..depth {
            // 每次读取 16 字节，stride 也是 16 字节的倍数
            let addr = mem_addr + (i * 16 * actual_stride);
            let value = self.read_u64(addr);
            // 写入 bank（按 8 字节偏移）
            let bank_offset = (i * 8) as usize;
            if bank_offset + 8 <= BANK_SIZE {
                self.banks[bank_id as usize][bank_offset..bank_offset + 8]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        
        0
    }

    // ==================== MVOUT 指令 (funct=25) ====================
    
    /// MVOUT: 从 bank 存储数据到内存
    /// 
    /// 宏定义：bb_mvout(mem_addr, bank_id, depth, stride)
    /// xs1 = BB_BANK0(bank_id) | BB_RD0 | FIELD(mem_addr, 27, 58)
    /// xs2 = FIELD(depth, 0, 9) | FIELD(stride, 10, 28)
    fn execute_mvout(&mut self, xs1: u64, xs2: u64) -> u64 {
        self.stats.mvout_count += 1;
        
        // 解码 xs1：提取 bank_id 和 mem_addr
        let bank_id = xs1 & 0x1F;              // bits 0-4
        let mem_addr = (xs1 >> 27) & 0xFFFFFFFF; // bits 27-58 (32 位地址)
        
        // 解码 xs2：提取 depth 和 stride
        let depth = xs2 & 0x3FF;              // bits 0-9
        let stride = (xs2 >> 10) & 0x7FFFF;   // bits 10-28
        
        info!(
            "MVOUT: mem_addr=0x{:x}, bank_id={}, depth={}, stride={}",
            mem_addr, bank_id, depth, stride
        );
        
        if bank_id >= BANK_NUM as u64 {
            error!("MVOUT: Invalid bank_id={}", bank_id);
            return 0;
        }
        
        if !self.bank_configs[bank_id as usize].allocated {
            error!("MVOUT: Bank {} not allocated", bank_id);
            return 0;
        }
        
        // 从 bank 读取数据写入内存
        // stride 按 16 字节（128 位）为单位
        let actual_stride = if stride == 0 { 1 } else { stride };
        for i in 0..depth {
            let bank_offset = (i * 8) as usize;
            if bank_offset + 8 > BANK_SIZE {
                break;
            }
            let value = u64::from_le_bytes(
                self.banks[bank_id as usize][bank_offset..bank_offset + 8]
                    .try_into().unwrap()
            );
            // 每次写入 16 字节，stride 也是 16 字节的倍数
            let addr = mem_addr + (i * 16 * actual_stride);
            self.write_u64(addr, value);
        }
        
        0
    }

    // ==================== MUL_WARP16 指令 (funct=32) ====================
    
    /// MUL_WARP16: 16x16 矩阵乘法
    /// 
    /// 宏定义：bb_mul_warp16(op1_bank_id, op2_bank_id, wr_bank_id, iter, mode)
    /// xs1 = BB_BANK0(op1_bank_id) | BB_BANK1(op2_bank_id) | BB_BANK2(wr_bank_id) | BB_RD0 | BB_RD1 | BB_WR
    /// xs2 = FIELD(iter, 0, 9) | FIELD(mode, 10, 63)
    fn execute_mul_warp16(&mut self, xs1: u64, xs2: u64) -> u64 {
        self.stats.matmul_count += 1;
        
        // 解码 xs1：提取三个 bank_id
        let op1_bank_id = xs1 & 0x1F;              // bits 0-4 (BANK0)
        let op2_bank_id = (xs1 >> 5) & 0x1F;       // bits 5-9 (BANK1)
        let wr_bank_id = (xs1 >> 10) & 0x1F;       // bits 10-14 (BANK2)
        
        // 解码 xs2：提取 iter 和 mode
        let iter = xs2 & 0x3FF;                    // bits 0-9
        let _mode = (xs2 >> 10) & 0xFFFFFFFFFFFFF; // bits 10-63
        
        info!(
            "MUL_WARP16: op1={}, op2={}, wr={}, iter={}",
            op1_bank_id, op2_bank_id, wr_bank_id, iter
        );
        
        // 验证 bank 有效性
        if op1_bank_id >= BANK_NUM as u64 || op2_bank_id >= BANK_NUM as u64 || wr_bank_id >= BANK_NUM as u64 {
            error!("MUL_WARP16: Invalid bank_id");
            return 0;
        }
        
        // 从 bank 读取矩阵 A 和 B（16x16，每个元素 8 字节）
        let matrix_a = self.read_matrix_from_bank(op1_bank_id);
        let matrix_b = self.read_matrix_from_bank(op2_bank_id);
        
        // 计算矩阵乘法 C = A × B
        let mut result = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                for k in 0..MATRIX_SIZE {
                    result[i][j] = result[i][j].wrapping_add(
                        matrix_a[i][k].wrapping_mul(matrix_b[k][j])
                    );
                }
            }
        }
        
        // 写入结果到 wr_bank
        self.write_matrix_to_bank(wr_bank_id, &result);
        
        info!("MUL_WARP16: Computed C = A × B, stored in bank {}", wr_bank_id);
        0
    }

    // ==================== TRANSPOSE 指令 (funct=34) ====================
    
    /// TRANSPOSE: 矩阵转置
    /// 
    /// 宏定义：bb_transpose(op1_bank_id, wr_bank_id, iter, mode)
    /// xs1 = BB_BANK0(op1_bank_id) | BB_BANK2(wr_bank_id) | BB_RD0 | BB_WR
    /// xs2 = FIELD(iter, 0, 9) | FIELD(mode, 10, 63)
    fn execute_transpose(&mut self, xs1: u64, xs2: u64) -> u64 {
        self.stats.transpose_count += 1;
        
        // 解码 xs1：提取 op1_bank_id 和 wr_bank_id
        let op1_bank_id = xs1 & 0x1F;        // bits 0-4 (BANK0)
        let wr_bank_id = (xs1 >> 10) & 0x1F; // bits 10-14 (BANK2)
        
        // 解码 xs2：提取 iter 和 mode
        let iter = xs2 & 0x3FF;              // bits 0-9
        let _mode = (xs2 >> 10) & 0xFFFFFFFFFFFFF; // bits 10-63
        
        info!(
            "TRANSPOSE: op1={}, wr={}, iter={}",
            op1_bank_id, wr_bank_id, iter
        );
        
        if op1_bank_id >= BANK_NUM as u64 || wr_bank_id >= BANK_NUM as u64 {
            error!("TRANSPOSE: Invalid bank_id");
            return 0;
        }
        
        // 从源 bank 读取矩阵
        let matrix = self.read_matrix_from_bank(op1_bank_id);
        
        // 计算转置
        let mut transposed = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                transposed[j][i] = matrix[i][j];
            }
        }
        
        // 写入目标 bank
        self.write_matrix_to_bank(wr_bank_id, &transposed);
        
        info!("TRANSPOSE: Transposed matrix from bank {} to bank {}", op1_bank_id, wr_bank_id);
        0
    }

    // ==================== 辅助函数 ====================
    
    /// 从 bank 读取 16x16 矩阵
    fn read_matrix_from_bank(&self, bank_id: u64) -> [[u64; MATRIX_SIZE]; MATRIX_SIZE] {
        let mut matrix = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                if offset + 8 <= BANK_SIZE {
                    matrix[i][j] = u64::from_le_bytes(
                        self.banks[bank_id as usize][offset..offset + 8]
                            .try_into().unwrap()
                    );
                }
            }
        }
        debug!("Read 16x16 matrix from bank {}", bank_id);
        matrix
    }
    
    /// 写入 16x16 矩阵到 bank
    fn write_matrix_to_bank(&mut self, bank_id: u64, matrix: &[[u64; MATRIX_SIZE]; MATRIX_SIZE]) {
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                if offset + 8 <= BANK_SIZE {
                    self.banks[bank_id as usize][offset..offset + 8]
                        .copy_from_slice(&matrix[i][j].to_le_bytes());
                }
            }
        }
        debug!("Wrote 16x16 matrix to bank {}", bank_id);
    }

    /// 从内存读取 u64
    fn read_u64(&self, addr: u64) -> u64 {
        let addr = addr as usize;
        if addr + 8 > self.memory.len() {
            error!("Read out of bounds: addr=0x{:x}", addr);
            return 0;
        }
        u64::from_le_bytes([
            self.memory[addr],
            self.memory[addr + 1],
            self.memory[addr + 2],
            self.memory[addr + 3],
            self.memory[addr + 4],
            self.memory[addr + 5],
            self.memory[addr + 6],
            self.memory[addr + 7],
        ])
    }

    /// 写入 u64 到内存
    fn write_u64(&mut self, addr: u64, value: u64) {
        let addr = addr as usize;
        if addr + 8 > self.memory.len() {
            error!("Write out of bounds: addr=0x{:x}", addr);
            return;
        }
        let bytes = value.to_le_bytes();
        for (i, &byte) in bytes.iter().enumerate() {
            self.memory[addr + i] = byte;
        }
    }

    /// 分配结果内存
    fn allocate_result(&mut self, size: usize) -> u64 {
        let addr = self.result_ptr;
        self.result_ptr += size as u64;
        if self.result_ptr >= self.memory.len() as u64 {
            self.result_ptr = 0x1000;
        }
        addr
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> &BemuStats {
        &self.stats
    }

    /// 重置统计信息
    pub fn reset_stats(&mut self) {
        self.stats = BemuStats::default();
    }

    /// 获取 bank 配置
    pub fn get_bank_config(&self, bank_id: u64) -> Option<&BankConfig> {
        if bank_id < BANK_NUM as u64 {
            Some(&self.bank_configs[bank_id as usize])
        } else {
            None
        }
    }

    /// 直接写入内存（用于测试初始化）
    pub fn write_memory(&mut self, addr: u64, data: &[u8]) {
        let addr = addr as usize;
        for (i, &byte) in data.iter().enumerate() {
            if addr + i < self.memory.len() {
                self.memory[addr + i] = byte;
            }
        }
    }

    /// 直接读取内存（用于测试验证）
    pub fn read_memory(&self, addr: u64, size: usize) -> &[u8] {
        let addr = addr as usize;
        &self.memory[addr..addr + size]
    }
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mset_alloc_release() {
        let mut bemu = Bemu::new();
        
        // 分配 bank 0: row=4, col=4
        bemu.execute_mset(0, 4 | (4 << 5) | (1 << 10));
        let config = bemu.get_bank_config(0).unwrap();
        assert_eq!(config.allocated, true);
        assert_eq!(config.rows, 4);
        assert_eq!(config.cols, 4);
        
        // 释放 bank 0
        bemu.execute_mset(0, 0);
        let config_released = bemu.get_bank_config(0).unwrap();
        assert_eq!(config_released.allocated, false);
        
        println!("✅ MSET alloc/release test passed!");
    }

    #[test]
    fn test_mvin_mvout() {
        let mut bemu = Bemu::new();
        
        // 分配 bank 0
        bemu.execute_mset(0, 1 | (1 << 5) | (1 << 10));
        
        // 准备测试数据（按 16 字节 stride 排列）
        let test_data = [0x1111111111111111u64, 0x2222222222222222, 0x3333333333333333];
        let mem_addr = 0x100u64;
        // 按 16 字节 stride 写入内存
        for (i, &val) in test_data.iter().enumerate() {
            let addr = mem_addr + (i * 16) as u64;  // stride=1 (16 字节)
            bemu.write_u64(addr, val);
        }
        
        // MVIN: 从内存加载到 bank 0
        // xs1 = BB_BANK0(bank_id) | BB_WR | FIELD(mem_addr, 27, 58)
        // bank_id=0, mem_addr=0x100
        let xs1 = 0 | (0x100 << 27);
        // xs2 = FIELD(depth, 0, 9) | FIELD(stride, 10, 28)
        // depth=3, stride=1 (16 字节为单位)
        let xs2 = 3 | (1 << 10);
        bemu.execute_mvin(xs1, xs2);
        
        // 验证 bank 中的数据
        for (i, &expected) in test_data.iter().enumerate() {
            let offset = i * 8;
            let actual = u64::from_le_bytes(
                bemu.banks[0][offset..offset + 8].try_into().unwrap()
            );
            assert_eq!(actual, expected, "Data mismatch at index {}", i);
        }
        
        // MVOUT: 从 bank 存储到内存
        let out_addr = 0x200u64;
        let xs1_out = 0 | (out_addr << 27);
        bemu.execute_mvout(xs1_out, xs2);
        
        // 验证内存中的数据（按 16 字节 stride 读取）
        for (i, &expected) in test_data.iter().enumerate() {
            let addr = out_addr + (i * 16) as u64;
            let actual = bemu.read_u64(addr);
            assert_eq!(actual, expected, "Output mismatch at index {}", i);
        }
        
        println!("✅ MVIN/MVOUT test passed!");
    }

    #[test]
    fn test_mul_warp16() {
        let mut bemu = Bemu::new();
        
        // 分配三个 bank
        bemu.execute_mset(0, 16 | (16 << 5) | (1 << 10)); // op1
        bemu.execute_mset(1, 16 | (16 << 5) | (1 << 10)); // op2
        bemu.execute_mset(2, 16 | (16 << 5) | (1 << 10)); // wr
        
        // 初始化单位矩阵到 bank 0
        for i in 0..MATRIX_SIZE {
            let offset = (i * MATRIX_SIZE + i) * 8;
            bemu.banks[0][offset..offset + 8].copy_from_slice(&1u64.to_le_bytes());
        }
        
        // 初始化 2I 矩阵到 bank 1
        for i in 0..MATRIX_SIZE {
            let offset = (i * MATRIX_SIZE + i) * 8;
            bemu.banks[1][offset..offset + 8].copy_from_slice(&2u64.to_le_bytes());
        }
        
        // 执行矩阵乘法：I × 2I = 2I
        let xs1 = 0 | (1 << 5) | (2 << 10); // op1=0, op2=1, wr=2
        let xs2 = 16; // iter=16
        bemu.execute_mul_warp16(xs1, xs2);
        
        // 验证结果
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let actual = u64::from_le_bytes(
                    bemu.banks[2][offset..offset + 8].try_into().unwrap()
                );
                let expected = if i == j { 2 } else { 0 };
                assert_eq!(actual, expected, "Mismatch at [{}][{}]", i, j);
            }
        }
        
        println!("✅ MUL_WARP16 test passed!");
    }

    #[test]
    fn test_transpose() {
        let mut bemu = Bemu::new();
        
        // 分配两个 bank
        bemu.execute_mset(0, 16 | (16 << 5) | (1 << 10)); // op1
        bemu.execute_mset(1, 16 | (16 << 5) | (1 << 10)); // wr
        
        // 初始化矩阵到 bank 0
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let value = ((i * MATRIX_SIZE + j) as u64) + 1;
                bemu.banks[0][offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        
        // 执行转置
        let xs1 = 0 | (1 << 10); // op1=0, wr=1
        let xs2 = 16; // iter=16
        bemu.execute_transpose(xs1, xs2);
        
        // 验证转置结果
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let actual = u64::from_le_bytes(
                    bemu.banks[1][offset..offset + 8].try_into().unwrap()
                );
                let expected = ((j * MATRIX_SIZE + i) as u64) + 1;
                assert_eq!(actual, expected, "Transpose mismatch at [{}][{}]", i, j);
            }
        }
        
        println!("✅ TRANSPOSE test passed!");
    }
}
