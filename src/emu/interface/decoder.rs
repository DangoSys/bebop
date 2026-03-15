/// 指令解码模块
/// 
/// 负责解析和解码 Buckyball 自定义指令
/// 支持的指令格式：
/// - R-R 格式：两个源操作数寄存器
/// - R 格式：单个源操作数寄存器

use log::{debug, info};

/// 指令解码结果
#[derive(Debug, Clone, Copy)]
pub struct DecodedInstruction {
    /// 功能码 (funct7)
    pub funct: u32,
    /// 源操作数 1 (xs1/rs1)
    pub xs1: u64,
    /// 源操作数 2 (xs2/rs2)
    pub xs2: u64,
    /// 指令类型
    pub instr_type: InstructionType,
}

/// 指令类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionType {
    /// MSET: 设置矩阵参数
    MSET,
    /// MVIN: 从内存加载到 bank
    MVIN,
    /// MVOUT: 从 bank 存储到内存
    MVOUT,
    /// MUL_WARP16: 16x16 矩阵乘法
    MUL_WARP16,
    /// TRANSPOSE: 矩阵转置
    TRANSPOSE,
    /// 未知指令
    Unknown,
}

impl InstructionType {
    /// 根据 funct 码判断指令类型
    pub fn from_funct(funct: u32) -> Self {
        match funct {
            23 => InstructionType::MSET,
            24 => InstructionType::MVIN,
            25 => InstructionType::MVOUT,
            32 => InstructionType::MUL_WARP16,
            34 => InstructionType::TRANSPOSE,
            _ => InstructionType::Unknown,
        }
    }
    
    /// 获取指令名称
    pub fn name(&self) -> &'static str {
        match self {
            InstructionType::MSET => "MSET",
            InstructionType::MVIN => "MVIN",
            InstructionType::MVOUT => "MVOUT",
            InstructionType::MUL_WARP16 => "MUL_WARP16",
            InstructionType::TRANSPOSE => "TRANSPOSE",
            InstructionType::Unknown => "UNKNOWN",
        }
    }
}

/// 指令解码器
pub struct InstructionDecoder {
    /// 是否启用详细日志
    verbose: bool,
}

impl InstructionDecoder {
    /// 创建新的指令解码器
    pub fn new() -> Self {
        Self { verbose: false }
    }
    
    /// 创建带详细日志的解码器
    pub fn with_verbose(verbose: bool) -> Self {
        Self { verbose }
    }
    
    /// 解码指令
    /// 
    /// # Arguments
    /// * `funct` - 功能码 (funct7)
    /// * `xs1` - 源操作数 1
    /// * `xs2` - 源操作数 2
    /// 
    /// # Returns
    /// * `DecodedInstruction` - 解码后的指令
    pub fn decode(&self, funct: u32, xs1: u64, xs2: u64) -> DecodedInstruction {
        let instr_type = InstructionType::from_funct(funct);
        
        if self.verbose {
            info!(
                "Decoding instruction: type={}, funct={}, xs1=0x{:x}, xs2=0x{:x}",
                instr_type.name(),
                funct,
                xs1,
                xs2
            );
        }
        
        DecodedInstruction {
            funct,
            xs1,
            xs2,
            instr_type,
        }
    }
    
    /// 从原始指令字解码（用于从二进制指令解码）
    /// 
    /// # Arguments
    /// * `instr` - 32 位指令字
    /// * `rs1_val` - rs1 寄存器值
    /// * `rs2_val` - rs2 寄存器值
    /// 
    /// # Returns
    /// * `DecodedInstruction` - 解码后的指令
    pub fn decode_from_instr_word(
        &self,
        instr: u32,
        rs1_val: u64,
        rs2_val: u64,
    ) -> DecodedInstruction {
        // RISC-V 指令格式解析
        // custom0: opcode=0x0b, funct7[31:25], rs2[24:20], rs1[19:15], funct3[14:12], rd[11:7]
        let funct7 = (instr >> 25) & 0x7F;
        let rs2 = (instr >> 20) & 0x1F;
        let rs1 = (instr >> 15) & 0x1F;
        let funct3 = (instr >> 12) & 0x07;
        let rd = (instr >> 7) & 0x1F;
        
        debug!(
            "Instruction word: 0x{:08x}, funct7={}, rs1={}, rs2={}, funct3={}, rd={}",
            instr, funct7, rs1, rs2, funct3, rd
        );
        
        // 对于 Buckyball 指令，funct7 作为 funct 码
        self.decode(funct7, rs1_val, rs2_val)
    }
    
    /// 验证指令是否有效
    pub fn is_valid(&self, decoded: &DecodedInstruction) -> bool {
        decoded.instr_type != InstructionType::Unknown
    }
    
    /// 获取指令的详细信息字符串
    pub fn instruction_details(&self, decoded: &DecodedInstruction) -> String {
        format!(
            "Instruction: {}\n  Funct: {}\n  XS1: 0x{:016x}\n  XS2: 0x{:016x}",
            decoded.instr_type.name(),
            decoded.funct,
            decoded.xs1,
            decoded.xs2
        )
    }
}

impl Default for InstructionDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_instruction_type_from_funct() {
        assert_eq!(InstructionType::from_funct(23), InstructionType::MSET);
        assert_eq!(InstructionType::from_funct(24), InstructionType::MVIN);
        assert_eq!(InstructionType::from_funct(25), InstructionType::MVOUT);
        assert_eq!(InstructionType::from_funct(32), InstructionType::MUL_WARP16);
        assert_eq!(InstructionType::from_funct(34), InstructionType::TRANSPOSE);
        assert_eq!(InstructionType::from_funct(99), InstructionType::Unknown);
    }
    
    #[test]
    fn test_instruction_type_name() {
        assert_eq!(InstructionType::MSET.name(), "MSET");
        assert_eq!(InstructionType::MVIN.name(), "MVIN");
        assert_eq!(InstructionType::MUL_WARP16.name(), "MUL_WARP16");
        assert_eq!(InstructionType::Unknown.name(), "UNKNOWN");
    }
    
    #[test]
    fn test_decoder_basic() {
        let decoder = InstructionDecoder::new();
        
        // 测试 MSET 指令解码
        let decoded = decoder.decode(23, 0, 4 | (4 << 5) | (1 << 10));
        assert_eq!(decoded.funct, 23);
        assert_eq!(decoded.instr_type, InstructionType::MSET);
        assert_eq!(decoded.xs1, 0);
        assert_eq!(decoded.xs2, 4 | (4 << 5) | (1 << 10));
        
        // 测试 MVIN 指令解码
        let decoded = decoder.decode(24, 0x100 << 27, 3 | (1 << 10));
        assert_eq!(decoded.funct, 24);
        assert_eq!(decoded.instr_type, InstructionType::MVIN);
    }
    
    #[test]
    fn test_decoder_validation() {
        let decoder = InstructionDecoder::new();
        
        let valid_instr = decoder.decode(23, 0, 0);
        assert!(decoder.is_valid(&valid_instr));
        
        let invalid_instr = decoder.decode(99, 0, 0);
        assert!(!decoder.is_valid(&invalid_instr));
    }
    
    #[test]
    fn test_instruction_details() {
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(23, 0x1234, 0x5678);
        
        let details = decoder.instruction_details(&decoded);
        assert!(details.contains("MSET"));
        assert!(details.contains("0x0000000000001234"));
        assert!(details.contains("0x0000000000005678"));
    }
}
