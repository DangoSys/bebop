use crate::compressed;
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
pub fn disassemble(inst: u32) -> String {
    let inst = if inst & 3 != 3 {
        match compressed::decode(inst as u16) {
            Some(expanded) => expanded,
            None => return format!(".hword 0x{:04x}", inst as u16),
        }
    } else {
        inst
    };
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
        0x07 if funct3 == 2 || funct3 == 3 => format!(
            "{} f{}, {}(x{})",
            if funct3 == 2 { "flw" } else { "fld" },
            rd,
            decode_itype_imm(inst) as i32,
            rs1
        ),
        0x27 if funct3 == 2 || funct3 == 3 => format!(
            "{} f{}, {}(x{})",
            if funct3 == 2 { "fsw" } else { "fsd" },
            rs2,
            decode_stype_imm(inst) as i32,
            rs1
        ),
        0x2f if funct3 == 2 || funct3 == 3 => {
            let mnemonic = match inst >> 27 {
                0 => "amoadd",
                1 => "amoswap",
                2 => "lr",
                3 => "sc",
                4 => "amoxor",
                8 => "amoor",
                12 => "amoand",
                16 => "amomin",
                20 => "amomax",
                24 => "amominu",
                28 => "amomaxu",
                _ => return format!(".word 0x{inst:08x}"),
            };
            let width = if funct3 == 2 { "w" } else { "d" };
            let order = match (inst >> 25) & 3 {
                0 => "",
                1 => ".rl",
                2 => ".aq",
                3 => ".aqrl",
                _ => unreachable!(),
            };
            if inst >> 27 == 2 {
                format!("{mnemonic}.{width}{order} x{rd}, (x{rs1})")
            } else {
                format!("{mnemonic}.{width}{order} x{rd}, x{rs2}, (x{rs1})")
            }
        }
        0x43 | 0x47 | 0x4b | 0x4f if (inst >> 25) & 3 <= 1 => {
            let op = match opcode {
                0x43 => "fmadd",
                0x47 => "fmsub",
                0x4b => "fnmsub",
                0x4f => "fnmadd",
                _ => unreachable!(),
            };
            let precision = if (inst >> 25) & 3 == 0 { "s" } else { "d" };
            format!("{op}.{precision} f{rd}, f{rs1}, f{rs2}, f{}, rm={funct3}", inst >> 27)
        }
        0x53 => {
            let precision = if funct7 & 1 == 0 { "s" } else { "d" };
            match funct7 {
                0x00 | 0x01 | 0x04 | 0x05 | 0x08 | 0x09 | 0x0c | 0x0d => {
                    let op = ["fadd", "fsub", "fmul", "fdiv"][(funct7 >> 2) as usize];
                    format!("{op}.{precision} f{rd}, f{rs1}, f{rs2}, rm={funct3}")
                }
                0x2c | 0x2d if rs2 == 0 => format!("fsqrt.{precision} f{rd}, f{rs1}, rm={funct3}"),
                0x10 | 0x11 if funct3 <= 2 => format!(
                    "{}.{precision} f{rd}, f{rs1}, f{rs2}",
                    ["fsgnj", "fsgnjn", "fsgnjx"][funct3 as usize]
                ),
                0x14 | 0x15 if funct3 <= 1 => format!(
                    "{}.{precision} f{rd}, f{rs1}, f{rs2}",
                    if funct3 == 0 { "fmin" } else { "fmax" }
                ),
                0x20 if rs2 == 1 => format!("fcvt.s.d f{rd}, f{rs1}, rm={funct3}"),
                0x21 if rs2 == 0 => format!("fcvt.d.s f{rd}, f{rs1}, rm={funct3}"),
                0x50 | 0x51 if funct3 <= 2 => format!(
                    "{}.{precision} x{rd}, f{rs1}, f{rs2}",
                    ["fle", "flt", "feq"][funct3 as usize]
                ),
                0x60 | 0x61 if rs2 <= 3 => format!(
                    "fcvt.{}.{precision} x{rd}, f{rs1}, rm={funct3}",
                    ["w", "wu", "l", "lu"][rs2]
                ),
                0x68 | 0x69 if rs2 <= 3 => format!(
                    "fcvt.{precision}.{} f{rd}, x{rs1}, rm={funct3}",
                    ["w", "wu", "l", "lu"][rs2]
                ),
                0x70 | 0x71 if rs2 == 0 && funct3 == 0 => {
                    format!("fmv.x.{} x{rd}, f{rs1}", if funct7 == 0x70 { "w" } else { "d" })
                }
                0x70 | 0x71 if rs2 == 0 && funct3 == 1 => format!("fclass.{precision} x{rd}, f{rs1}"),
                0x78 | 0x79 if rs2 == 0 && funct3 == 0 => {
                    format!("fmv.{}.x f{rd}, x{rs1}", if funct7 == 0x78 { "w" } else { "d" })
                }
                _ => format!(".word 0x{inst:08x}"),
            }
        }
        0x57 if funct3 == 7 => {
            if inst >> 25 == 0x40 {
                format!("vsetvl x{rd}, x{rs1}, x{rs2}")
            } else if inst >> 31 == 0 || inst >> 30 == 3 {
                let vt = (inst >> 20) & 255;
                let op = if inst >> 30 == 3 { "vsetivli" } else { "vsetvli" };
                let avl = if op == "vsetivli" {
                    rs1.to_string()
                } else {
                    format!("x{rs1}")
                };
                format!(
                    "{op} x{rd}, {avl}, e{}, {}, {}, {}",
                    8 << ((vt >> 3) & 7),
                    ["m1", "m2", "m4", "m8", "reserved", "mf8", "mf4", "mf2"][(vt & 7) as usize],
                    if vt & 64 != 0 { "ta" } else { "tu" },
                    if vt & 128 != 0 { "ma" } else { "mu" }
                )
            } else {
                format!(".word 0x{inst:08x}")
            }
        }
        0x73 => {
            // System
            match funct3 {
                0x0 if inst == 0x00000073 => "ecall".to_string(),
                0x0 if inst == 0x00100073 => "ebreak".to_string(),
                0x0 if inst == 0x10200073 => "sret".to_string(),
                0x0 if inst == 0x30200073 => "mret".to_string(),
                0x0 if inst == 0x10500073 => "wfi".to_string(),
                0x0 if inst & 0xfe007fff == 0x12000073 => format!("sfence.vma x{rs1}, x{rs2}"),
                0x1 => format!("csrrw x{}, 0x{:x}, x{}", rd, (inst >> 20) & 0xfff, rs1),
                0x2 => format!("csrrs x{}, 0x{:x}, x{}", rd, (inst >> 20) & 0xfff, rs1),
                0x3 => format!("csrrc x{}, 0x{:x}, x{}", rd, (inst >> 20) & 0xfff, rs1),
                0x5 => format!("csrrwi x{}, 0x{:x}, {}", rd, (inst >> 20) & 0xfff, rs1),
                0x6 => format!("csrrsi x{}, 0x{:x}, {}", rd, (inst >> 20) & 0xfff, rs1),
                0x7 => format!("csrrci x{}, 0x{:x}, {}", rd, (inst >> 20) & 0xfff, rs1),
                _ => format!("system? 0x{:08x}", inst),
            }
        }
        0x0f if funct3 == 0 => "fence".to_string(),
        0x0f if funct3 == 1 => "fence.i".to_string(),
        _ => format!(".word 0x{inst:08x}"),
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
