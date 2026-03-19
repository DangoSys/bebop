/// Spike 回调接口模块
/// 
/// 提供与 Spike 模拟器集成的标准化接口
/// 包含：
/// - 回调函数 trait 定义
/// - 参数和返回值类型定义
/// - 错误处理机制
/// - 日志输出

use log::{debug, error, info};
use crate::emu::bemu::Bemu;
use crate::emu::config::BemuStats;

/// Spike 回调函数结果类型
pub type SpikeResult = Result<u64, SpikeError>;

/// Spike 错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpikeError {
    /// 未知指令
    UnknownInstruction(u32),
    /// 无效的内存访问
    InvalidMemoryAccess(u64),
    /// Bank 未分配
    BankNotAllocated(u64),
    /// 参数错误
    InvalidParameter(String),
    /// 内部错误
    InternalError(String),
}

impl std::fmt::Display for SpikeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpikeError::UnknownInstruction(funct) => {
                write!(f, "Unknown instruction: funct={}", funct)
            }
            SpikeError::InvalidMemoryAccess(addr) => {
                write!(f, "Invalid memory access: addr=0x{:x}", addr)
            }
            SpikeError::BankNotAllocated(bank_id) => {
                write!(f, "Bank not allocated: bank_id={}", bank_id)
            }
            SpikeError::InvalidParameter(msg) => {
                write!(f, "Invalid parameter: {}", msg)
            }
            SpikeError::InternalError(msg) => {
                write!(f, "Internal error: {}", msg)
            }
        }
    }
}

impl std::error::Error for SpikeError {}

/// Spike 回调函数参数
#[derive(Debug, Clone)]
pub struct SpikeCallbackParams {
    /// 功能码
    pub funct: u32,
    /// 源操作数 1
    pub xs1: u64,
    /// 源操作数 2
    pub xs2: u64,
    /// 程序计数器（可选）
    pub pc: Option<u64>,
    /// 时间戳（可选）
    pub timestamp: Option<u64>,
}

impl SpikeCallbackParams {
    /// 创建新的回调参数
    pub fn new(funct: u32, xs1: u64, xs2: u64) -> Self {
        Self {
            funct,
            xs1,
            xs2,
            pc: None,
            timestamp: None,
        }
    }
    
    /// 设置程序计数器
    pub fn with_pc(mut self, pc: u64) -> Self {
        self.pc = Some(pc);
        self
    }
    
    /// 设置时间戳
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }
}

/// Spike 回调函数 trait
/// 
/// 这是 Spike 与 BEMU 之间的主要接口
/// Spike 通过调用这些回调函数来执行自定义指令
pub trait SpikeCallbacks {
    /// 处理自定义指令
    /// 
    /// # Arguments
    /// * `params` - 回调参数
    /// 
    /// # Returns
    /// * `SpikeResult` - 执行结果或错误
    fn handle_custom_instruction(&mut self, params: &SpikeCallbackParams) -> SpikeResult;
    
    /// 同步内存（从 Spike 到 BEMU）
    /// 
    /// # Arguments
    /// * `addr` - 内存地址
    /// * `data` - 数据
    /// 
    /// # Returns
    /// * `Result<(), SpikeError>` - 成功或错误
    fn sync_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), SpikeError>;
    
    /// 获取统计信息
    /// 
    /// # Returns
    /// * `&BemuStats` - 统计信息
    fn get_stats(&self) -> &BemuStats;
    
    /// 重置统计信息
    fn reset_stats(&mut self);
    
    /// 获取 BEMU 版本信息
    fn get_version(&self) -> &'static str {
        "0.1.0"
    }
}

/// BEMU Spike 接口实现
pub struct BemuSpikeInterface {
    /// BEMU 实例
    bemu: Bemu,
    /// 是否启用详细日志
    verbose: bool,
    /// 指令执行计数器
    instruction_count: u64,
}

impl BemuSpikeInterface {
    /// 创建新的 BEMU Spike 接口
    pub fn new() -> Self {
        Self {
            bemu: Bemu::new(),
            verbose: false,
            instruction_count: 0,
        }
    }
    
    /// 创建带详细日志的接口
    pub fn with_verbose(verbose: bool) -> Self {
        let mut bemu = Bemu::new();
        bemu.set_verbose(verbose);
        Self {
            bemu,
            verbose,
            instruction_count: 0,
        }
    }
    
    /// 获取 BEMU 实例的不可变引用
    pub fn get_bemu(&self) -> &Bemu {
        &self.bemu
    }
    
    /// 获取 BEMU 实例的可变引用
    pub fn get_bemu_mut(&mut self) -> &mut Bemu {
        &mut self.bemu
    }
    
    /// 从内存读取数据（BEMU 内地址按 512KB 取模）
    pub fn read_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, SpikeError> {
        let _ = addr.checked_add(size as u64).ok_or_else(|| SpikeError::InvalidMemoryAccess(addr))?;
        Ok(self.bemu.read_memory(addr, size))
    }
    
    /// 执行指令并记录日志
    fn execute_with_logging(&mut self, funct: u32, xs1: u64, xs2: u64) -> SpikeResult {
        self.instruction_count += 1;
        
        if self.verbose {
            info!(
                "Executing instruction #{}: funct={}, xs1=0x{:x}, xs2=0x{:x}",
                self.instruction_count, funct, xs1, xs2
            );
        }
        
        // 执行指令
        let result = self.bemu.execute(funct, xs1, xs2);
        
        if self.verbose {
            info!("Instruction #{} completed, result=0x{:x}", self.instruction_count, result);
        }
        
        Ok(result)
    }
}

impl Default for BemuSpikeInterface {
    fn default() -> Self {
        Self::new()
    }
}

impl SpikeCallbacks for BemuSpikeInterface {
    fn handle_custom_instruction(&mut self, params: &SpikeCallbackParams) -> SpikeResult {
        if self.verbose {
            debug!(
                "Handling custom instruction: funct={}, xs1=0x{:x}, xs2=0x{:x}, pc={:?}",
                params.funct, params.xs1, params.xs2, params.pc
            );
        }
        
        // 直接执行指令
        match self.execute_with_logging(params.funct, params.xs1, params.xs2) {
            Ok(result) => {
                if self.verbose {
                    debug!("Instruction executed successfully, result=0x{:x}", result);
                }
                Ok(result)
            }
            Err(e) => {
                error!("Instruction execution failed: {:?}", e);
                Err(e)
            }
        }
    }
    
    fn sync_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), SpikeError> {
        if self.verbose {
            debug!("Syncing memory: addr=0x{:x}, size={}", addr, data.len());
        }
        
        // 直接写入 BEMU 内存（假设地址已经是虚拟地址）
        self.bemu.write_memory(addr, data);
        Ok(())
    }
    
    fn get_stats(&self) -> &BemuStats {
        self.bemu.get_stats()
    }
    
    fn reset_stats(&mut self) {
        self.bemu.reset_stats();
        self.instruction_count = 0;
        info!("BEMU statistics reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_spike_callback_params() {
        let params = SpikeCallbackParams::new(23, 0x100, 0x200)
            .with_pc(0x1000)
            .with_timestamp(12345);
        
        assert_eq!(params.funct, 23);
        assert_eq!(params.xs1, 0x100);
        assert_eq!(params.xs2, 0x200);
        assert_eq!(params.pc, Some(0x1000));
        assert_eq!(params.timestamp, Some(12345));
    }
    
    #[test]
    fn test_spike_error_display() {
        let err = SpikeError::UnknownInstruction(99);
        assert_eq!(format!("{}", err), "Unknown instruction: funct=99");
        
        let err = SpikeError::InvalidMemoryAccess(0x1000);
        assert_eq!(format!("{}", err), "Invalid memory access: addr=0x1000");
    }
    
    #[test]
    fn test_bemu_spike_interface_creation() {
        let interface = BemuSpikeInterface::new();
        assert_eq!(interface.get_version(), "0.1.0");
    }
    
    #[test]
    fn test_bemu_spike_interface_handle_instruction() {
        let mut interface = BemuSpikeInterface::with_verbose(true);
        
        // 测试 MSET 指令
        let params = SpikeCallbackParams::new(23, 0, 4 | (4 << 5) | (1 << 10));
        let result = interface.handle_custom_instruction(&params);
        assert!(result.is_ok());
        
        // 未知指令：BEMU 返回 u64::MAX，Spike 扩展可据此抛 illegal_instruction
        let params = SpikeCallbackParams::new(99, 0, 0);
        let result = interface.handle_custom_instruction(&params);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), u64::MAX);
    }
    
    #[test]
    fn test_bemu_spike_interface_memory_sync() {
        let mut interface = BemuSpikeInterface::new();
        
        let test_data = [0x11, 0x22, 0x33, 0x44];
        let result = interface.sync_memory(0x100, &test_data);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_bemu_spike_interface_stats() {
        let mut interface = BemuSpikeInterface::new();
        
        // 执行一些指令
        let params1 = SpikeCallbackParams::new(23, 0, 1 | (1 << 5) | (1 << 10));
        let _ = interface.handle_custom_instruction(&params1);
        
        let params2 = SpikeCallbackParams::new(23, 1, 1 | (1 << 5) | (1 << 10));
        let _ = interface.handle_custom_instruction(&params2);
        
        // 检查统计
        let stats = interface.get_stats();
        assert_eq!(stats.instructions_executed, 2);
        
        // 重置统计
        interface.reset_stats();
        let stats = interface.get_stats();
        assert_eq!(stats.instructions_executed, 0);
    }
}
