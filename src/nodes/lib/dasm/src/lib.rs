// RISC-V disassembler for DASM(...) patterns

use std::io::{BufRead, Write};

/// Process input line by line, replacing DASM(hex) with disassembled instruction
pub fn process_dasm<R: BufRead, W: Write>(reader: R, mut writer: W) -> std::io::Result<()> {
    for line in reader.lines() {
        let line = line?;
        let processed = process_line(&line);
        writeln!(writer, "{}", processed)?;
    }
    Ok(())
}

/// Process a single line, replacing all DASM(hex) patterns
fn process_line(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut pos = 0;

    while let Some(start) = line[pos..].find("DASM(") {
        let start = pos + start;
        result.push_str(&line[pos..start]);

        let mut end = start + 5; // "DASM(".len()

        // Skip optional 0x prefix
        if line.len() > end + 1 && line.as_bytes()[end] == b'0' {
            let next = line.as_bytes()[end + 1];
            if next == b'x' || next == b'X' {
                end += 2;
            }
        }

        // Find hex digits
        let hex_start = end;
        while end < line.len() && line.as_bytes()[end].is_ascii_hexdigit() {
            end += 1;
        }

        // Check for closing paren
        if end < line.len() && line.as_bytes()[end] == b')' {
            if let Ok(bits) = u32::from_str_radix(&line[hex_start..end], 16) {
                let dis = disassemble(bits);
                result.push_str(&dis);
                pos = end + 1;
                continue;
            }
        }

        // If parsing failed, keep original
        result.push_str(&line[start..end.min(line.len())]);
        pos = end;
    }

    result.push_str(&line[pos..]);
    result
}

/// Disassemble a RISC-V instruction
fn disassemble(inst: u32) -> String {
    let opcode = inst & 0x7f;
    let rd = ((inst >> 7) & 0x1f) as usize;
    let funct3 = (inst >> 12) & 0x7;
    let rs1 = ((inst >> 15) & 0x1f) as usize;
    let rs2 = ((inst >> 20) & 0x1f) as usize;
    let funct7 = inst >> 25;

    match opcode {
        0x37 => format!("lui x{}, 0x{:x}", rd, inst >> 12),   // LUI
        0x17 => format!("auipc x{}, 0x{:x}", rd, inst >> 12), // AUIPC
        0x6f => {
            // JAL
            let imm = decode_jtype_imm(inst);
            format!("jal x{}, {}", rd, imm as i32)
        }
        0x67 => {
            // JALR
            let imm = decode_itype_imm(inst);
            format!("jalr x{}, {}(x{})", rd, imm as i32, rs1)
        }
        0x63 => {
            // Branch
            let imm = decode_btype_imm(inst);
            let mnemonic = match funct3 {
                0x0 => "beq",
                0x1 => "bne",
                0x4 => "blt",
                0x5 => "bge",
                0x6 => "bltu",
                0x7 => "bgeu",
                _ => "branch?",
            };
            format!("{} x{}, x{}, {}", mnemonic, rs1, rs2, imm as i32)
        }
        0x03 => {
            // Load
            let imm = decode_itype_imm(inst);
            let mnemonic = match funct3 {
                0x0 => "lb",
                0x1 => "lh",
                0x2 => "lw",
                0x3 => "ld",
                0x4 => "lbu",
                0x5 => "lhu",
                0x6 => "lwu",
                _ => "load?",
            };
            format!("{} x{}, {}(x{})", mnemonic, rd, imm as i32, rs1)
        }
        0x23 => {
            // Store
            let imm = decode_stype_imm(inst);
            let mnemonic = match funct3 {
                0x0 => "sb",
                0x1 => "sh",
                0x2 => "sw",
                0x3 => "sd",
                _ => "store?",
            };
            format!("{} x{}, {}(x{})", mnemonic, rs2, imm as i32, rs1)
        }
        0x13 => {
            // I-type ALU
            let imm = decode_itype_imm(inst);
            let mnemonic = match funct3 {
                0x0 => "addi",
                0x1 => "slli",
                0x2 => "slti",
                0x3 => "sltiu",
                0x4 => "xori",
                0x5 if funct7 == 0x00 => "srli",
                0x5 if funct7 == 0x20 => "srai",
                0x6 => "ori",
                0x7 => "andi",
                _ => "alui?",
            };
            format!("{} x{}, x{}, {}", mnemonic, rd, rs1, imm as i32)
        }
        0x1b => {
            // I-type ALU (32-bit)
            let imm = decode_itype_imm(inst);
            let mnemonic = match funct3 {
                0x0 => "addiw",
                0x1 => "slliw",
                0x5 if funct7 == 0x00 => "srliw",
                0x5 if funct7 == 0x20 => "sraiw",
                _ => "aluiw?",
            };
            format!("{} x{}, x{}, {}", mnemonic, rd, rs1, imm as i32)
        }
        0x33 => {
            // R-type ALU
            let mnemonic = match (funct7, funct3) {
                (0x00, 0x0) => "add",
                (0x20, 0x0) => "sub",
                (0x00, 0x1) => "sll",
                (0x00, 0x2) => "slt",
                (0x00, 0x3) => "sltu",
                (0x00, 0x4) => "xor",
                (0x00, 0x5) => "srl",
                (0x20, 0x5) => "sra",
                (0x00, 0x6) => "or",
                (0x00, 0x7) => "and",
                (0x01, 0x0) => "mul",
                (0x01, 0x1) => "mulh",
                (0x01, 0x2) => "mulhsu",
                (0x01, 0x3) => "mulhu",
                (0x01, 0x4) => "div",
                (0x01, 0x5) => "divu",
                (0x01, 0x6) => "rem",
                (0x01, 0x7) => "remu",
                _ => "alu?",
            };
            format!("{} x{}, x{}, x{}", mnemonic, rd, rs1, rs2)
        }
        0x3b => {
            // R-type ALU (32-bit)
            let mnemonic = match (funct7, funct3) {
                (0x00, 0x0) => "addw",
                (0x20, 0x0) => "subw",
                (0x00, 0x1) => "sllw",
                (0x00, 0x5) => "srlw",
                (0x20, 0x5) => "sraw",
                (0x01, 0x0) => "mulw",
                (0x01, 0x4) => "divw",
                (0x01, 0x5) => "divuw",
                (0x01, 0x6) => "remw",
                (0x01, 0x7) => "remuw",
                _ => "aluw?",
            };
            format!("{} x{}, x{}, x{}", mnemonic, rd, rs1, rs2)
        }
        0x73 => {
            // System
            match funct3 {
                0x0 if inst == 0x00000073 => "ecall".to_string(),
                0x0 if inst == 0x00100073 => "ebreak".to_string(),
                0x0 if inst == 0x10200073 => "sret".to_string(),
                0x0 if inst == 0x30200073 => "mret".to_string(),
                0x1 => format!("csrrw x{}, 0x{:x}, x{}", rd, (inst >> 20) & 0xfff, rs1),
                0x2 => format!("csrrs x{}, 0x{:x}, x{}", rd, (inst >> 20) & 0xfff, rs1),
                0x3 => format!("csrrc x{}, 0x{:x}, x{}", rd, (inst >> 20) & 0xfff, rs1),
                0x5 => format!("csrrwi x{}, 0x{:x}, {}", rd, (inst >> 20) & 0xfff, rs1),
                0x6 => format!("csrrsi x{}, 0x{:x}, {}", rd, (inst >> 20) & 0xfff, rs1),
                0x7 => format!("csrrci x{}, 0x{:x}, {}", rd, (inst >> 20) & 0xfff, rs1),
                _ => format!("system? 0x{:08x}", inst),
            }
        }
        0x0f => "fence".to_string(), // FENCE
        _ => format!("unknown 0x{:08x}", inst),
    }
}

fn decode_itype_imm(inst: u32) -> u32 {
    ((inst as i32) >> 20) as u32
}

fn decode_stype_imm(inst: u32) -> u32 {
    let imm11_5 = (inst >> 25) & 0x7f;
    let imm4_0 = (inst >> 7) & 0x1f;
    let imm = (imm11_5 << 5) | imm4_0;
    ((imm as i32) << 20 >> 20) as u32
}

fn decode_btype_imm(inst: u32) -> u32 {
    let imm12 = (inst >> 31) & 0x1;
    let imm10_5 = (inst >> 25) & 0x3f;
    let imm4_1 = (inst >> 8) & 0xf;
    let imm11 = (inst >> 7) & 0x1;
    let imm = (imm12 << 12) | (imm11 << 11) | (imm10_5 << 5) | (imm4_1 << 1);
    ((imm as i32) << 19 >> 19) as u32
}

fn decode_jtype_imm(inst: u32) -> u32 {
    let imm20 = (inst >> 31) & 0x1;
    let imm10_1 = (inst >> 21) & 0x3ff;
    let imm11 = (inst >> 20) & 0x1;
    let imm19_12 = (inst >> 12) & 0xff;
    let imm = (imm20 << 20) | (imm19_12 << 12) | (imm11 << 11) | (imm10_1 << 1);
    ((imm as i32) << 11 >> 11) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_line() {
        let line = "C0: 19 [1] pc=[0x10040] inst=[020005b7] DASM(020005b7)";
        let result = process_line(line);
        assert!(result.contains("lui"));
    }
}
