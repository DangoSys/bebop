/// Bemu - Bebop Emulator (软件模型/Golden Model)
/// 作用：
/// 1. 用软件快速实现自定义指令的语义
/// 2. 作为 Golden Model（参考模型）
/// 3. 与 RTL 实现对比（Difftest）
/// 
/// 根据 Buckyball C 代码宏定义实现指令语义：
/// - MSET (funct=23): 分配/释放内存 bank
/// - MVIN (funct=24): 从内存加载数据到 bank
/// - MVOUT (funct=25): 从 bank 存储数据到内存
/// - MUL_WARP16 (funct=32): 16x16 矩阵乘法
/// - TRANSPOSE (funct=34): 矩阵转置

use log::{debug, error, info};
use super::config::{BANK_NUM, BANK_SIZE, TOTAL_MEMORY_SIZE, BankConfig, BemuStats};
use super::instructions::mset;
use super::instructions::mvin;
use super::instructions::mvout;
use super::instructions::matmul;

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
    
    /// 设置详细日志模式
    pub fn set_verbose(&mut self, verbose: bool) {
        if verbose {
            log::set_max_level(log::LevelFilter::Debug);
        } else {
            log::set_max_level(log::LevelFilter::Info);
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
    /// * `u64` - 执行结果：
    ///   - 成功：返回 funct 码（例如 MSET 返回 23）
    ///   - 失败：返回 0
    ///   - 未知指令：返回 u64::MAX
    pub fn execute(&mut self, funct: u32, xs1: u64, xs2: u64) -> u64 {
        self.stats.instructions_executed += 1;
        
        debug!("Bemu executing: funct={}, xs1=0x{:x}, xs2=0x{:x}", funct, xs1, xs2);
        
        let result = match funct {
            // Buckyball 核心指令
            23 => {
                let ret = mset::execute_mset(xs1, xs2, &mut self.bank_configs, &mut self.banks);
                if ret == 0 { funct as u64 } else { 0u64 }  // 成功返回 funct
            },
            24 => {
                let ret = mvin::execute_mvin(xs1, xs2, &self.memory, &mut self.banks, &self.bank_configs);
                if ret == 0 { funct as u64 } else { 0u64 }
            },
            25 => {
                let ret = mvout::execute_mvout(xs1, xs2, &mut self.memory, &self.banks, &self.bank_configs);
                if ret == 0 { funct as u64 } else { 0u64 }
            },
            32 => {
                let ret = matmul::execute_mul_warp16(xs1, xs2, &mut self.banks);
                if ret == 0 { funct as u64 } else { 0u64 }
            },
            34 => {
                let ret = matmul::execute_transpose(xs1, xs2, &mut self.banks);
                if ret == 0 { funct as u64 } else { 0u64 }
            },
            
            _ => {
                error!("Bemu: Unknown funct={}", funct);
                u64::MAX  // 未知指令返回特殊值
            }
        };
        
        debug!("Bemu result: 0x{:x}", result);
        result
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

    /// 直接写入内存。地址按 512KB 取模，使 Spike guest 任意 VA 可映射到 BEMU 空间。
    pub fn write_memory(&mut self, addr: u64, data: &[u8]) {
        let len = self.memory.len();
        for (i, &byte) in data.iter().enumerate() {
            let idx = ((addr as usize) + i) % len;
            self.memory[idx] = byte;
        }
    }

    /// 直接读取内存，返回 Vec。地址按 512KB 取模。
    pub fn read_memory(&self, addr: u64, size: usize) -> Vec<u8> {
        let len = self.memory.len();
        (0..size)
            .map(|i| self.memory[((addr as usize) + i) % len])
            .collect()
    }

    /// 写入 u64 到内存（用于测试）
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

    /// 读取 u64 从内存（用于测试）
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
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::MATRIX_SIZE;

    #[test]
    fn test_mset_alloc_release() {
        let mut bemu = Bemu::new();
        
        // 分配 bank 0: row=4, col=4
        bemu.execute(23, 0, 4 | (4 << 5) | (1 << 10));
        let config = bemu.get_bank_config(0).unwrap();
        assert_eq!(config.allocated, true);
        assert_eq!(config.rows, 4);
        assert_eq!(config.cols, 4);
        
        // 释放 bank 0
        bemu.execute(23, 0, 0);
        let config_released = bemu.get_bank_config(0).unwrap();
        assert_eq!(config_released.allocated, false);
        
        println!("MSET alloc/release test passed!");
    }

    #[test]
    fn test_mvin_mvout() {
        let mut bemu = Bemu::new();
        
        // 分配 bank 0
        bemu.execute(23, 0, 1 | (1 << 5) | (1 << 10));
        
        // 准备测试数据（16 字节块格式，每个块包含 2 个 u64）
        let test_values: [(u64, u64); 3] = [
            (0x1111111111111111u64, 0xAAAAAAAAAAAAAAAAu64),
            (0x2222222222222222u64, 0xBBBBBBBBBBBBBBBBu64),
            (0x3333333333333333u64, 0xCCCCCCCCCCCCCCCCu64),
        ];
        let mem_addr = 0x100u64;
        // 按 16 字节块写入内存
        for (i, &(low, high)) in test_values.iter().enumerate() {
            let addr = mem_addr + (i as u64 * 16);
            bemu.write_u64(addr, low);
            bemu.write_u64(addr + 8, high);
        }
        
        // MVIN: 从内存加载到 bank 0
        let xs1 = 0 | (0x100 << 27);
        let xs2 = 3 | (1 << 10);
        bemu.execute(24, xs1, xs2);
        
        // 验证 bank 中的数据（每个块 16 字节）
        for (i, &(expected_low, expected_high)) in test_values.iter().enumerate() {
            let offset = i * 16;
            let actual_low = u64::from_le_bytes(
                bemu.banks[0][offset..offset + 8].try_into().unwrap()
            );
            let actual_high = u64::from_le_bytes(
                bemu.banks[0][offset + 8..offset + 16].try_into().unwrap()
            );
            assert_eq!(actual_low, expected_low, "Low mismatch at index {}", i);
            assert_eq!(actual_high, expected_high, "High mismatch at index {}", i);
        }
        
        // MVOUT: 从 bank 存储到内存
        let out_addr = 0x200u64;
        let xs1_out = 0 | (out_addr << 27);
        bemu.execute(25, xs1_out, xs2);
        
        // 验证内存中的数据（每个块 16 字节）
        for (i, &(expected_low, expected_high)) in test_values.iter().enumerate() {
            let addr = out_addr + (i * 16) as u64;
            let actual_low = bemu.read_u64(addr);
            let actual_high = bemu.read_u64(addr + 8);
            assert_eq!(actual_low, expected_low, "Low mismatch at index {}", i);
            assert_eq!(actual_high, expected_high, "High mismatch at index {}", i);
        }
        
        println!("MVIN/MVOUT test passed!");
    }

    #[test]
    fn test_mul_warp16() {
        let mut bemu = Bemu::new();
        
        // 分配三个 bank
        bemu.execute(23, 0, 16 | (16 << 5) | (1 << 10)); // op1
        bemu.execute(23, 1, 16 | (16 << 5) | (1 << 10)); // op2
        bemu.execute(23, 2, 16 | (16 << 5) | (1 << 10)); // wr
        
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
        bemu.execute(32, xs1, xs2);
        
        // 验证结果
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let actual = u64::from_le_bytes(
                    bemu.banks[2][offset..offset + 8].try_into().unwrap()
                );
                let expected = if i == j { 2 } else { 0 };
                assert_eq!(actual, expected, 
                    "Mismatch at [{}][{}]: expected {}, got {}", i, j, expected, actual);
            }
        }
        
        println!("MUL_WARP16 test passed!");
    }

    #[test]
    fn test_transpose() {
        let mut bemu = Bemu::new();
        
        // 分配两个 bank
        bemu.execute(23, 0, 16 | (16 << 5) | (1 << 10)); // op1
        bemu.execute(23, 1, 16 | (16 << 5) | (1 << 10)); // wr
        
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
        bemu.execute(34, xs1, xs2);
        
        // 验证转置结果
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let actual = u64::from_le_bytes(
                    bemu.banks[1][offset..offset + 8].try_into().unwrap()
                );
                let expected = ((j * MATRIX_SIZE + i) as u64) + 1;
                assert_eq!(actual, expected, 
                    "Transpose mismatch at [{}][{}]: expected {}, got {}", i, j, expected, actual);
            }
        }
        
        println!("TRANSPOSE test passed!");
    }
}
