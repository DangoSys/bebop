//! An instruction decoded once, where it is fetched: the base integer instructions that make up most of a
//! program carry their operation and operands, every other instruction is executed from its bits.

use super::bitmanip;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Op {
    /// Executed from the instruction bits.
    Other,
    Lui,
    Auipc,
    Jal,
    Jalr,
    Beq,
    Bne,
    Blt,
    Bge,
    Bltu,
    Bgeu,
    Lb,
    Lh,
    Lw,
    Ld,
    Lbu,
    Lhu,
    Lwu,
    Sb,
    Sh,
    Sw,
    Sd,
    Addi,
    Slti,
    Sltiu,
    Xori,
    Ori,
    Andi,
    Slli,
    Srli,
    Srai,
    Addiw,
    Slliw,
    Srliw,
    Sraiw,
    Add,
    Sub,
    Sll,
    Slt,
    Sltu,
    Xor,
    Srl,
    Sra,
    Or,
    And,
    Addw,
    Subw,
    Sllw,
    Srlw,
    Sraw,
}

#[derive(Clone, Copy)]
pub(super) struct Decoded {
    /// The instruction as fetched.
    pub raw: u32,
    /// Its 4-byte form: itself, or what its compressed form stands for. Zero is no instruction.
    pub insn: u32,
    pub op: Op,
    pub rd: u8,
    pub rs1: u8,
    pub rs2: u8,
    /// The immediate of the operation, sign-extended: its offset, its shift amount or its upper immediate.
    pub imm: i32,
}

impl Decoded {
    pub const NONE: Self = Self {
        raw: 0,
        insn: 0,
        op: Op::Other,
        rd: 0,
        rs1: 0,
        rs2: 0,
        imm: 0,
    };

    pub fn new(raw: u32, insn: u32) -> Self {
        let funct = (insn >> 12) & 7;
        let top = insn >> 25;
        let immediate = (insn as i32) >> 20;
        let upper = (insn & 0xfffff000) as i32;
        let other = (Op::Other, 0);
        let (op, imm) = match insn & 0x7f {
            // An encoding the bit-manipulation extensions take is theirs.
            0x13 | 0x1b | 0x33 | 0x3b if top != 0 && bitmanip::execute(insn, 0, 0, 64).is_some() => other,
            0x37 => (Op::Lui, upper),
            0x17 => (Op::Auipc, upper),
            0x6f => {
                let offset =
                    (insn & 0xff000) | ((insn >> 9) & 0x800) | ((insn >> 20) & 0x7fe) | ((insn >> 11) & 0x100000);
                (Op::Jal, (offset << 11) as i32 >> 11)
            }
            0x67 if funct == 0 => (Op::Jalr, immediate),
            0x63 => {
                let offset =
                    ((insn >> 19) & 0x1000) | ((insn << 4) & 0x800) | ((insn >> 20) & 0x7e0) | ((insn >> 7) & 0x1e);
                let offset = (offset << 19) as i32 >> 19;
                match funct {
                    0 => (Op::Beq, offset),
                    1 => (Op::Bne, offset),
                    4 => (Op::Blt, offset),
                    5 => (Op::Bge, offset),
                    6 => (Op::Bltu, offset),
                    7 => (Op::Bgeu, offset),
                    _ => other,
                }
            }
            0x03 => match funct {
                0 => (Op::Lb, immediate),
                1 => (Op::Lh, immediate),
                2 => (Op::Lw, immediate),
                3 => (Op::Ld, immediate),
                4 => (Op::Lbu, immediate),
                5 => (Op::Lhu, immediate),
                6 => (Op::Lwu, immediate),
                _ => other,
            },
            0x23 => {
                let offset = (((insn >> 7) & 31) | ((insn >> 20) & 0xfe0)) as i32;
                let offset = (offset << 20) >> 20;
                match funct {
                    0 => (Op::Sb, offset),
                    1 => (Op::Sh, offset),
                    2 => (Op::Sw, offset),
                    3 => (Op::Sd, offset),
                    _ => other,
                }
            }
            0x13 => match funct {
                0 => (Op::Addi, immediate),
                2 => (Op::Slti, immediate),
                3 => (Op::Sltiu, immediate),
                4 => (Op::Xori, immediate),
                6 => (Op::Ori, immediate),
                7 => (Op::Andi, immediate),
                1 if insn >> 26 == 0 => (Op::Slli, immediate & 63),
                5 if insn >> 26 == 0 => (Op::Srli, immediate & 63),
                5 if insn >> 26 == 0x10 => (Op::Srai, immediate & 63),
                _ => other,
            },
            0x1b => match funct {
                0 => (Op::Addiw, immediate),
                1 if top == 0 => (Op::Slliw, immediate & 31),
                5 if top == 0 => (Op::Srliw, immediate & 31),
                5 if top == 0x20 => (Op::Sraiw, immediate & 31),
                _ => other,
            },
            0x33 => match (top, funct) {
                (0, 0) => (Op::Add, 0),
                (0x20, 0) => (Op::Sub, 0),
                (0, 1) => (Op::Sll, 0),
                (0, 2) => (Op::Slt, 0),
                (0, 3) => (Op::Sltu, 0),
                (0, 4) => (Op::Xor, 0),
                (0, 5) => (Op::Srl, 0),
                (0x20, 5) => (Op::Sra, 0),
                (0, 6) => (Op::Or, 0),
                (0, 7) => (Op::And, 0),
                _ => other,
            },
            0x3b => match (top, funct) {
                (0, 0) => (Op::Addw, 0),
                (0x20, 0) => (Op::Subw, 0),
                (0, 1) => (Op::Sllw, 0),
                (0, 5) => (Op::Srlw, 0),
                (0x20, 5) => (Op::Sraw, 0),
                _ => other,
            },
            _ => other,
        };
        Self {
            raw,
            insn,
            op,
            rd: ((insn >> 7) & 31) as u8,
            rs1: ((insn >> 15) & 31) as u8,
            rs2: ((insn >> 20) & 31) as u8,
            imm,
        }
    }
}
