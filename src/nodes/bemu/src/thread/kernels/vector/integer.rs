use super::super::{Engine, Fault};

impl Engine {
    pub(crate) fn vector_integer(&mut self) -> Result<(), Fault> {
        let insn = self.instruction;
        let illegal = self.illegal();
        let vd = ((insn >> 7) & 31) as usize;
        let kind = (insn >> 12) & 7;
        let vs1 = ((insn >> 15) & 31) as usize;
        let vs2 = ((insn >> 20) & 31) as usize;
        let masked = insn & (1 << 25) == 0;
        let op = insn >> 26;
        let vector = &self.vector;
        if kind == 3 && op == 0x27 {
            let count = vs1 + 1;
            if masked
                || !matches!(count, 1 | 2 | 4 | 8)
                || !vd.is_multiple_of(count)
                || !vs2.is_multiple_of(count)
                || vd + count > 32
                || vs2 + count > 32
                || vector.vstart != 0
            {
                return Err(illegal);
            }
            let bytes = count * vector.vlen / 8;
            let source = vs2 * vector.vlen / 8;
            let destination = vd * vector.vlen / 8;
            self.vector.registers.copy_within(source..source + bytes, destination);
            return Ok(());
        }
        if kind == 2 && matches!(op, 0x18..=0x1f) {
            if masked {
                return Err(illegal);
            }
            for index in vector.vstart..vector.vl {
                let a = self.vector.mask(vs2, index);
                let b = self.vector.mask(vs1, index);
                let value = match op {
                    0x18 => a && !b,
                    0x19 => a && b,
                    0x1a => a || b,
                    0x1b => a ^ b,
                    0x1c => a || !b,
                    0x1d => !(a && b),
                    0x1e => !(a || b),
                    0x1f => !(a ^ b),
                    _ => unreachable!(),
                };
                self.vector.write_mask(vd, index, value);
            }
            return Ok(());
        }
        if kind == 2 && op == 0x10 && vs1 == 0 {
            if masked {
                return Err(illegal);
            }
            let bits = vector.sew();
            self.x[vd] = (((vector.read(vs2, 0, bits) << (64 - bits)) as i64) >> (64 - bits)) as u64;
            return Ok(());
        }
        if kind == 6 && op == 0x10 {
            if masked || vs2 != 0 {
                return Err(illegal);
            }
            if vector.vstart < vector.vl {
                let bits = vector.sew();
                let value = self.x[vs1];
                self.vector.write(vd, 0, bits, value);
            }
            return Ok(());
        }
        if kind == 2 && op <= 7 {
            if vector.vstart != 0 || !vector.group(vs2, vector.sew()) {
                return Err(illegal);
            }
            if vector.vl != 0 {
                let bits = vector.sew();
                let mask = u64::MAX >> (64 - bits);
                let mut result = vector.read(vs1, 0, bits);
                for index in 0..vector.vl {
                    if !masked || vector.mask(0, index) {
                        let value = vector.read(vs2, index, bits);
                        result = match op {
                            0 => result.wrapping_add(value) & mask,
                            1 => result & value,
                            2 => result | value,
                            3 => result ^ value,
                            4 => result.min(value),
                            5 => {
                                let a = ((result << (64 - bits)) as i64) >> (64 - bits);
                                let b = ((value << (64 - bits)) as i64) >> (64 - bits);
                                if a < b {
                                    result
                                } else {
                                    value
                                }
                            }
                            6 => result.max(value),
                            7 => {
                                let a = ((result << (64 - bits)) as i64) >> (64 - bits);
                                let b = ((value << (64 - bits)) as i64) >> (64 - bits);
                                if a > b {
                                    result
                                } else {
                                    value
                                }
                            }
                            _ => unreachable!(),
                        };
                    }
                }
                self.vector.write(vd, 0, bits, result);
            }
            return Ok(());
        }
        if kind == 2 && op == 0x10 && vs1 == 0x10 {
            if vector.vstart != 0 {
                return Err(illegal);
            }
            self.x[vd] = (0..vector.vl)
                .filter(|&i| vector.mask(vs2, i) && (!masked || vector.mask(0, i)))
                .count() as u64;
            return Ok(());
        }
        if kind == 2 && op == 0x14 && vs1 == 0x11 && vs2 == 0 {
            if !vector.group(vd, vector.sew()) || (masked && vd == 0) {
                return Err(illegal);
            }
            let bits = vector.sew();
            for index in vector.vstart..vector.vl {
                if !masked || self.vector.mask(0, index) {
                    self.vector.write(vd, index, bits, index as u64);
                }
            }
            return Ok(());
        }
        if kind == 2 && op == 0x12 {
            if !(2..=7).contains(&vs1) {
                return Err(illegal);
            }
            let bits = vector.sew();
            let factor = 1usize << (4 - vs1 / 2);
            let source_bits = bits / factor;
            if source_bits < 8 || !vector.group(vd, bits) || !vector.group(vs2, source_bits) || (masked && vd == 0) {
                return Err(illegal);
            }
            let register_bytes = vector.vlen / 8;
            let destination_bytes = vector.vlmax() * bits / 8;
            let source_bytes = destination_bytes / factor;
            let destination_begin = vd * register_bytes;
            let source_begin = vs2 * register_bytes;
            let destination_end = destination_begin + destination_bytes;
            let source_end = source_begin + source_bytes;
            if source_begin < destination_end
                && destination_begin < source_end
                && (source_bytes < register_bytes || source_end != destination_end)
            {
                return Err(illegal);
            }
            for index in vector.vstart..vector.vl {
                if !masked || self.vector.mask(0, index) {
                    let value = self.vector.read(vs2, index, source_bits);
                    let extended = if vs1 & 1 == 0 {
                        value
                    } else {
                        (((value << (64 - source_bits)) as i64) >> (64 - source_bits)) as u64
                    };
                    self.vector.write(vd, index, bits, extended);
                }
            }
            return Ok(());
        }
        let bits = vector.sew();
        let widening = matches!(kind, 2 | 6) && op == 0x38;
        let narrowing = matches!(kind, 0 | 3 | 4) && op == 0x2c;
        let source_bits = if narrowing { bits * 2 } else { bits };
        let destination_bits = if widening { bits * 2 } else { bits };
        if source_bits > vector.elen || destination_bits > vector.elen {
            return Err(illegal);
        }
        let mask = u64::MAX >> (64 - destination_bits);
        let compare = matches!(op, 0x18..=0x1f);
        let vector_operand = matches!(kind, 0 | 2);
        let merge = op == 0x17;
        let multiply = matches!(kind, 2 | 6);
        if (!compare && !vector.group(vd, destination_bits))
            || !vector.group(vs2, source_bits)
            || (vector_operand && !vector.group(vs1, bits))
            || (masked && vd == 0 && !compare)
            || (merge && !masked && vs2 != 0)
            || if multiply {
                !matches!(op, 0x20..=0x25 | 0x38)
            } else {
                !matches!(op, 0 | 2 | 3 | 4..=7 | 9..=11 | 0x17..=0x1f | 0x25 | 0x28 | 0x29 | 0x2c)
            }
            || !matches!(kind, 0 | 2 | 3 | 4 | 6)
        {
            return Err(illegal);
        }
        let scalar = if kind == 3 {
            ((vs1 as i64) << 59 >> 59) as u64
        } else {
            self.x[vs1]
        };
        for index in vector.vstart..vector.vl {
            let vector = &mut self.vector;
            let selected = !masked || vector.mask(0, index);
            if masked && !selected && !merge {
                continue;
            }
            let a = vector.read(vs2, index, source_bits);
            let b = if vector_operand {
                vector.read(vs1, index, bits)
            } else {
                scalar & (u64::MAX >> (64 - bits))
            };
            let signed_a = ((a << (64 - bits)) as i64) >> (64 - bits);
            let signed_b = ((b << (64 - bits)) as i64) >> (64 - bits);
            let value = match op {
                0 => a.wrapping_add(b),
                2 => a.wrapping_sub(b),
                3 => b.wrapping_sub(a),
                4 => a.min(b),
                5 => signed_a.min(signed_b) as u64,
                6 => a.max(b),
                7 => signed_a.max(signed_b) as u64,
                9 => a & b,
                10 => a | b,
                11 => a ^ b,
                0x17 => {
                    if !masked || selected {
                        b
                    } else {
                        a
                    }
                }
                0x18 => u64::from(a == b),
                0x19 => u64::from(a != b),
                0x1a => u64::from(a < b),
                0x1b => u64::from(signed_a < signed_b),
                0x1c => u64::from(a <= b),
                0x1d => u64::from(signed_a <= signed_b),
                0x1e => u64::from(a > b),
                0x1f => u64::from(signed_a > signed_b),
                0x20 if multiply => {
                    if b == 0 {
                        mask
                    } else {
                        a / b
                    }
                }
                0x21 if multiply => {
                    if b == 0 {
                        mask
                    } else {
                        signed_a.wrapping_div(signed_b) as u64
                    }
                }
                0x22 if multiply => {
                    if b == 0 {
                        a
                    } else {
                        a % b
                    }
                }
                0x23 if multiply => {
                    if b == 0 {
                        a
                    } else {
                        signed_a.wrapping_rem(signed_b) as u64
                    }
                }
                0x24 if multiply => ((a as u128 * b as u128) >> bits) as u64,
                0x25 | 0x38 if multiply => a.wrapping_mul(b),
                0x25 => a << (b & (bits as u64 - 1)),
                0x28 | 0x2c => a >> (b & (source_bits as u64 - 1)),
                0x29 => (signed_a >> (b & (bits as u64 - 1))) as u64,
                _ => unreachable!(),
            };
            if compare {
                vector.write_mask(vd, index, value != 0);
            } else {
                vector.write(vd, index, destination_bits, value & mask);
            }
        }
        Ok(())
    }
}
