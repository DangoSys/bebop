fn i(opcode: u32, rd: u32, funct: u32, rs: u32, imm: u32) -> u32 {
    (imm & 0xfff) << 20 | rs << 15 | funct << 12 | rd << 7 | opcode
}

fn s(opcode: u32, funct: u32, base: u32, src: u32, imm: u32) -> u32 {
    (imm >> 5) << 25 | src << 20 | base << 15 | funct << 12 | (imm & 31) << 7 | opcode
}

pub(super) fn decode(raw: u16) -> Option<u32> {
    let c = u32::from(raw);
    let rd = (c >> 7) & 31;
    let rs2 = (c >> 2) & 31;
    let small_rd = 8 + ((c >> 2) & 7);
    let small_rs = 8 + ((c >> 7) & 7);
    let imm6 = ((c >> 2) & 31) | ((c >> 7) & 32);
    let signed6 = ((imm6 << 26) as i32 >> 26) as u32;
    let ld_offset = ((c >> 7) & 0x38) | ((c << 1) & 0xc0);
    let lw_offset = ((c >> 7) & 0x38) | ((c >> 4) & 4) | ((c << 1) & 0x40);
    Some(match (c & 3, c >> 13) {
        (0, 0) => {
            let imm = ((c >> 1) & 0x3c0) | ((c >> 7) & 0x30) | ((c >> 2) & 8) | ((c >> 4) & 4);
            if imm == 0 {
                return None;
            }
            i(0x13, small_rd, 0, 2, imm)
        }
        (0, 1) => i(0x07, small_rd, 3, small_rs, ld_offset),
        (0, 2) => i(0x03, small_rd, 2, small_rs, lw_offset),
        (0, 3) => i(0x03, small_rd, 3, small_rs, ld_offset),
        (0, 5) => s(0x27, 3, small_rs, small_rd, ld_offset),
        (0, 6) => s(0x23, 2, small_rs, small_rd, lw_offset),
        (0, 7) => s(0x23, 3, small_rs, small_rd, ld_offset),
        (1, 0) => i(0x13, rd, 0, rd, signed6),
        (1, 1) if rd != 0 => i(0x1b, rd, 0, rd, signed6),
        (1, 2) => i(0x13, rd, 0, 0, signed6),
        (1, 3) if rd == 2 => {
            let imm =
                ((c >> 3) & 0x200) | ((c >> 2) & 0x10) | ((c << 1) & 0x40) | ((c << 4) & 0x180) | ((c << 3) & 0x20);
            if imm == 0 {
                return None;
            }
            i(0x13, 2, 0, 2, ((imm << 22) as i32 >> 22) as u32)
        }
        (1, 3) if imm6 != 0 => (signed6 << 12) | rd << 7 | 0x37,
        (1, 4) => match (c >> 10) & 3 {
            0 => i(0x13, small_rs, 5, small_rs, imm6),
            1 => i(0x13, small_rs, 5, small_rs, imm6 | 0x400),
            2 => i(0x13, small_rs, 7, small_rs, signed6),
            3 => {
                let op = (c >> 5) & 3;
                let word = c & 0x1000 != 0;
                let (funct7, funct3) = match (word, op) {
                    (false, 0) | (true, 0) => (0x20, 0),
                    (false, 1) => (0, 4),
                    (false, 2) => (0, 6),
                    (false, 3) => (0, 7),
                    (true, 1) => (0, 0),
                    _ => return None,
                };
                funct7 << 25
                    | small_rd << 20
                    | small_rs << 15
                    | funct3 << 12
                    | small_rs << 7
                    | if word { 0x3b } else { 0x33 }
            }
            _ => unreachable!(),
        },
        (1, 5) => {
            let imm = ((c >> 1) & 0x800)
                | ((c >> 7) & 0x10)
                | ((c >> 1) & 0x300)
                | ((c << 2) & 0x400)
                | ((c >> 1) & 0x40)
                | ((c << 1) & 0x80)
                | ((c >> 2) & 0xe)
                | ((c << 3) & 0x20);
            let imm = ((imm << 20) as i32 >> 20) as u32;
            ((imm >> 20) & 1) << 31 | ((imm >> 1) & 0x3ff) << 21 | ((imm >> 11) & 1) << 20 | (imm & 0xff000) | 0x6f
        }
        (1, 6 | 7) => {
            let imm = ((c >> 4) & 0x100) | ((c >> 7) & 0x18) | ((c << 1) & 0xc0) | ((c >> 2) & 6) | ((c << 3) & 0x20);
            let imm = ((imm << 23) as i32 >> 23) as u32;
            ((imm >> 12) & 1) << 31
                | ((imm >> 5) & 0x3f) << 25
                | small_rs << 15
                | ((c >> 13) & 1) << 12
                | ((imm >> 1) & 15) << 8
                | ((imm >> 11) & 1) << 7
                | 0x63
        }
        (2, 0) => i(0x13, rd, 1, rd, imm6),
        (2, 1 | 3) => {
            if c >> 13 == 3 && rd == 0 {
                return None;
            }
            let imm = ((c >> 7) & 0x20) | ((c >> 2) & 0x18) | ((c << 4) & 0x1c0);
            i(if c >> 13 == 1 { 0x07 } else { 0x03 }, rd, 3, 2, imm)
        }
        (2, 2) if rd != 0 => {
            let imm = ((c >> 7) & 0x20) | ((c >> 2) & 0x1c) | ((c << 4) & 0xc0);
            i(0x03, rd, 2, 2, imm)
        }
        (2, 4) => match (c & 0x1000 != 0, rs2, rd) {
            (false, 0, 0) => return None,
            (false, 0, _) => i(0x67, 0, 0, rd, 0),
            (false, _, _) => rs2 << 20 | rd << 7 | 0x33,
            (true, 0, 0) => 0x00100073,
            (true, 0, _) => i(0x67, 1, 0, rd, 0),
            (true, _, _) => rs2 << 20 | rd << 15 | rd << 7 | 0x33,
        },
        (2, 5 | 7) => {
            let imm = ((c >> 7) & 0x38) | ((c >> 1) & 0x1c0);
            s(if c >> 13 == 5 { 0x27 } else { 0x23 }, 3, 2, rs2, imm)
        }
        (2, 6) => {
            let imm = ((c >> 7) & 0x3c) | ((c >> 1) & 0xc0);
            s(0x23, 2, 2, rs2, imm)
        }
        _ => return None,
    })
}
