use super::operator_memory::{CONSTANT_BASE, STACK_BASE, STACK_END};
use super::Engine;
use std::collections::BTreeMap;

/// The inputs of the cycles of a vector instruction whose cost depends on no element value (Engine::timing_key).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TimingKey {
    instruction: u32,
    vl: usize,
    vstart: usize,
    vtype: u32,
    mask: Vec<u64>,
    address: Option<(u64, u64, bool, bool)>,
    waiting: u64,
    vrf_delta: u64,
    operands: u32,
}

/// The hasher of the timing memo. Its keys are simulator state, not outside input, so a rotate
/// and a multiply per word spread them enough; SipHash was a tenth of a model's host time.
#[derive(Default)]
pub struct KeyHasher(u64);

impl std::hash::Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(value.into());
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(value.into());
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

/// Its cycles and what it leaves: the VRF write end after the current cycle and the state of the operand cache.
#[derive(Clone, Copy)]
pub struct TimingResult {
    cycles: u64,
    vrf_delta: Option<u64>,
    operands: u32,
}

fn floating_latency(a: u64, b: u64, bits: usize, sqrt: bool) -> u64 {
    let a = a & (u64::MAX >> (64 - bits));
    let b = b & (u64::MAX >> (64 - bits));
    let (fraction_bits, exponent_bits, bias) = if bits == 32 { (23, 8, 127) } else { (52, 11, 1023) };
    let exponent_mask = (1u64 << exponent_bits) - 1;
    let fraction_mask = (1u64 << fraction_bits) - 1;
    let exponent = |value: u64| (value >> fraction_bits) & exponent_mask;
    let zero = |value: u64| exponent(value) == 0 && value & fraction_mask == 0;
    let special = |value: u64| exponent(value) == exponent_mask || zero(value);
    if special(a) || (sqrt && a >> (bits - 1) != 0) || (!sqrt && special(b)) {
        return 1;
    }
    if sqrt {
        let power = if exponent(a) == 0 {
            1 - bias - ((a & fraction_mask).leading_zeros() as i32 - (63 - fraction_bits as i32))
        } else {
            exponent(a) as i32 - bias
        };
        return (fraction_bits + 1 + usize::from(power & 1 == 0)) as u64;
    }
    let significand = |value: u64| {
        let fraction = value & fraction_mask;
        if exponent(value) != 0 {
            fraction | (1u64 << fraction_bits)
        } else {
            fraction << (fraction.leading_zeros() - (63 - fraction_bits as u32))
        }
    };
    (fraction_bits + 3) as u64 - u64::from(significand(a) >= significand(b))
}

fn local(address: u64) -> bool {
    (CONSTANT_BASE..STACK_END).contains(&address)
}

impl Engine {
    fn operand_read_cycles(&mut self, addresses: &[usize]) -> u64 {
        let banks = self.lanes.min(self.vector.vlen / self.vector.elen);
        let mut misses = vec![Vec::new(); banks];
        for &address in addresses {
            let requests = &mut misses[address % banks];
            if !self.operand_cache.contains(&Some(address)) && !requests.contains(&address) {
                requests.push(address);
            }
        }
        let rounds = misses.iter().map(Vec::len).max().unwrap();
        for round in 0..rounds {
            for requests in &misses {
                if let Some(&address) = requests.get(round) {
                    self.operand_cache[self.operand_cache_next] = Some(address);
                    self.operand_cache_next = (self.operand_cache_next + 1) % self.operand_cache.len();
                }
            }
        }
        // Banks take a request a cycle (arch: Operands, VRF): the busiest bank's accesses, then the last response,
        // answered the cycle it comes.
        rounds as u64 + 2
    }

    /// Event costs of the GP execution IP, including VRF clearing, operand
    /// collection, memory arbitration and masked word commits. One-cycle scalar
    /// dispatch, VRF reads and two-cycle commits writing behind the next instruction are calibrated from
    /// the Goban RTL instruction/state waveform, not emulator wall time.
    pub fn instruction_cycles(&mut self) -> u64 {
        let insn = self.instruction;
        let opcode = insn & 127;
        let kind = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let vector_memory = matches!(opcode, 7 | 39) && matches!(kind, 0 | 5 | 6 | 7);
        if opcode != 0x57 && !vector_memory {
            if matches!(opcode, 3 | 7 | 35 | 39) {
                let store = matches!(opcode, 35 | 39);
                let immediate = if store {
                    (((insn >> 25) << 5 | ((insn >> 7) & 31)) as i32) << 20 >> 20
                } else {
                    (insn as i32) >> 20
                };
                let address = self.x[rs1].wrapping_add(immediate as i64 as u64);
                let bytes = 1usize << (kind & 3);
                let row_bytes = if local(address) { 8 } else { 16 };
                let crossing = address as usize % row_bytes + bytes > row_bytes;
                return 1
                    + if local(address) {
                        if store {
                            2
                        } else {
                            3
                        }
                    } else {
                        6
                    }
                    + u64::from(crossing);
            }
            if opcode == 0x53 {
                let operation = (insn >> 25) & !3;
                if operation == 12 || operation == 0x2c {
                    let bits = if insn & (1 << 25) == 0 { 32 } else { 64 };
                    return 1 + floating_latency(self.f[rs1], self.f[rs2], bits, operation == 0x2c);
                }
            }
            if matches!(opcode, 0x33 | 0x3b) && insn >> 25 == 1 && kind >= 4 {
                return 3 + if self.x[rs2] == 0 {
                    0
                } else if opcode == 0x3b {
                    32
                } else {
                    64
                };
            }
            return 1;
        }
        match self.timing_key() {
            Some(key) => {
                if let Some(&result) = self.timing_memo.get(&key) {
                    if let Some(delta) = result.vrf_delta {
                        self.vrf_ready = self.cycles + delta;
                    }
                    // The cache itself follows only when an instruction is timed in full.
                    if self.operand_state != Some(result.operands) {
                        self.operand_state = Some(result.operands);
                        self.operand_stale = true;
                    }
                    return result.cycles;
                }
                self.load_operand_cache();
                let vrf_ready = self.vrf_ready;
                let cycles = self.vector_cycles();
                self.operand_state = None;
                let result = TimingResult {
                    cycles,
                    vrf_delta: (self.vrf_ready != vrf_ready).then(|| self.vrf_ready - self.cycles),
                    operands: self.operand_state(),
                };
                self.timing_memo.insert(key, result);
                cycles
            }
            None => {
                self.load_operand_cache();
                self.operand_state = None;
                self.vector_cycles()
            }
        }
    }

    /// The id of the operand cache's state: its entries from the oldest to the newest, which decide its future hits
    /// and evictions. A memo key holds the id, not the entries, so a memoized instruction costs no pass over them.
    fn operand_state(&mut self) -> u32 {
        if let Some(id) = self.operand_state {
            return id;
        }
        let entries = self.operand_cache.len();
        let state: Vec<Option<usize>> = (0..entries)
            .map(|slot| self.operand_cache[(self.operand_cache_next + slot) % entries])
            .collect();
        let id = match self.operand_state_ids.get(&state) {
            Some(&id) => id,
            None => {
                let id = self.operand_states.len() as u32;
                self.operand_states.push(state.clone());
                self.operand_state_ids.insert(state, id);
                id
            }
        };
        self.operand_state = Some(id);
        id
    }

    /// Brings the operand cache to the state memoized instructions left it in.
    fn load_operand_cache(&mut self) {
        if self.operand_stale {
            let id = self.operand_state.expect("a stale operand cache has a known state");
            self.operand_cache.clone_from(&self.operand_states[id as usize]);
            self.operand_cache_next = 0;
            self.operand_stale = false;
        }
    }

    /// Everything the cycles of a vector instruction depend on, for those whose cost does not depend on the
    /// element values or per-element addresses: the instruction, the vector state, the mask, the class of the base
    /// address, the cycles still to wait, and the operand cache. None for the rest (permutes, indexed and strided
    /// accesses, data-dependent floating latencies), which are always timed in full.
    fn timing_key(&mut self) -> Option<TimingKey> {
        let insn = self.instruction;
        let opcode = insn & 127;
        let kind = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let op = insn >> 26;
        let vector_memory = matches!(opcode, 7 | 39) && matches!(kind, 0 | 5 | 6 | 7);
        let fp = opcode == 0x57 && matches!(kind, 1 | 5);
        let integer = opcode == 0x57 && matches!(kind, 0 | 2 | 3 | 4 | 6);
        let multiply = matches!(kind, 2 | 6);
        let permute =
            integer && !multiply && (op == 12 || matches!(op, 14 | 15)) || integer && multiply && matches!(op, 14 | 15);
        let floating = fp && (matches!(op, 32 | 33) || op == 19 && rs1 == 0);
        let mode = (insn >> 26) & 3;
        if permute || floating || vector_memory && mode != 0 {
            return None;
        }
        let vm = insn & (1 << 25) != 0;
        let mask = if vm {
            Vec::new()
        } else {
            // Only the mask bits of the elements the instruction covers decide its cycles. The rest of v0 is
            // whatever the register holds: in the key it would make every execution a new entry.
            let elements = if integer && kind == 3 && op == 39 {
                self.vector.vlen
            } else {
                self.vector.vl
            };
            let mut bits = vec![0u64; elements.div_ceil(64)];
            for index in 0..elements {
                if self.vector.mask(0, index) {
                    bits[index / 64] |= 1 << (index % 64);
                }
            }
            bits
        };
        let address = if vector_memory {
            let base = self.x[rs1];
            let bytes = (((insn >> 29) as usize + 1) * self.vector.vlen / 8) as u64;
            let last = base.wrapping_add(bytes.max((self.vector.vl * 8) as u64) - 1);
            if local(base) != local(last) || base >> 32 != last >> 32 {
                return None;
            }
            let fits = (base & 0xffff_ffff) + bytes <= self.bank_bytes as u64
                || (base >= STACK_BASE && base + bytes <= STACK_END);
            Some((base >> 32, base & 15, local(base), fits))
        } else {
            None
        };
        let ready = self.vector.vlen / self.vector.elen * 32 / self.lanes.min(self.vector.vlen / self.vector.elen) + 1;
        Some(TimingKey {
            instruction: insn,
            vl: self.vector.vl,
            vstart: self.vector.vstart,
            vtype: self.vector.vtype,
            mask,
            address,
            waiting: (ready as u64).saturating_sub(self.cycles + 1),
            vrf_delta: self.vrf_ready.saturating_sub(self.cycles),
            operands: self.operand_state(),
        })
    }

    fn vector_cycles(&mut self) -> u64 {
        let insn = self.instruction;
        let opcode = insn & 127;
        let kind = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let vector_memory = matches!(opcode, 7 | 39) && matches!(kind, 0 | 5 | 6 | 7);
        let ready = self.vector.vlen / self.vector.elen * 32 / self.lanes.min(self.vector.vlen / self.vector.elen) + 1;
        let waiting = (ready as u64).saturating_sub(self.cycles + 1);
        if opcode == 0x57 && kind == 7 {
            return waiting + 4;
        }
        let op = insn >> 26;
        let vm = insn & (1 << 25) != 0;
        let fp = opcode == 0x57 && matches!(kind, 1 | 5);
        let integer = opcode == 0x57 && matches!(kind, 0 | 2 | 3 | 4 | 6);
        let multiply = matches!(kind, 2 | 6);
        let vector_operand = matches!(kind, 0..=2);
        let scalar_out = op == 16 && rs1 == 0 && vm && matches!(kind, 1 | 2);
        let scalar_in = op == 16 && rs2 == 0 && vm && matches!(kind, 5 | 6);
        let population = integer && kind == 2 && op == 16 && rs1 == 16;
        let first = integer && kind == 2 && op == 16 && rs1 == 17;
        let identity = integer && kind == 2 && op == 20 && rs1 == 17 && rs2 == 0;
        let mask_logical = integer && kind == 2 && (24..=31).contains(&op);
        let int_reduction = integer && kind == 2 && op <= 7;
        let fp_reduction = fp && kind == 1 && matches!(op, 1 | 3 | 5 | 7);
        let reduction = int_reduction || fp_reduction;
        let merge = op == 23 && (fp || integer && !multiply);
        let whole_move = integer && kind == 3 && op == 39;
        let store = opcode == 39;
        let mode = (insn >> 26) & 3;
        let whole_memory = vector_memory && mode == 0 && rs2 == 8;
        let indexed = vector_memory && mode & 1 != 0 && !whole_memory;
        let memory_bits = match kind {
            0 => 8,
            5 => 16,
            6 => 32,
            7 => 64,
            _ => 0,
        };
        let bytes = ((insn >> 29) as usize + 1) * self.vector.vlen / 8;
        let base_address = self.x[rs1];
        let word_transfer = whole_memory
            && (self.vector.vstart * memory_bits / 8).is_multiple_of(self.vector.elen / 8)
            && ((base_address & 0xffff_ffff) + bytes as u64 <= self.bank_bytes as u64
                || (base_address >= STACK_BASE && base_address + bytes as u64 <= STACK_END));
        let transfer_bits = if word_transfer { self.vector.elen } else { memory_bits };
        let sew = self.vector.sew();
        let element_bits = if indexed { sew } else { transfer_bits };
        let end = if whole_memory {
            bytes * 8 / transfer_bits
        } else if whole_move {
            (rs1 + 1) * self.vector.vlen / sew
        } else {
            self.vector.vl
        };
        let begin = if whole_memory {
            self.vector.vstart * memory_bits / transfer_bits
        } else {
            self.vector.vstart
        };
        let gather = integer && !multiply && (op == 12 || op == 14 && kind == 0);
        let slide = integer && !multiply && matches!(op, 14 | 15) && kind != 0;
        let slide_one = integer && multiply && matches!(op, 14 | 15);
        let permute = gather || slide || slide_one;
        let wide = integer && multiply && op >= 48 || fp && op == 18 && (8..=15).contains(&rs1);
        let narrow = integer && !multiply && (44..=47).contains(&op) || fp && op == 18 && (16..=23).contains(&rs1);
        let source_bits = if vector_memory {
            if indexed {
                memory_bits
            } else {
                element_bits
            }
        } else if narrow || integer && multiply && (52..=55).contains(&op) {
            sew * 2
        } else if integer && multiply && op == 18 {
            sew / match (rs1 >> 1) & 3 {
                1 => 8,
                2 => 4,
                _ => 2,
            }
        } else {
            sew
        };
        let operand_needed = !vector_memory
            && vector_operand
            && !reduction
            && !scalar_out
            && !population
            && !first
            && !(fp && matches!(op, 18 | 19))
            && !(integer && multiply && matches!(op, 18 | 20));
        let source_needed = if vector_memory {
            indexed
        } else {
            !scalar_in && !identity && !(merge && vm)
        };
        let destination_needed = if vector_memory {
            store
        } else {
            (fp || integer && multiply) && op >= 40
        };
        // Unit-stride transfers to the banks go a 16-byte bank row a port,
        // `row_elements` elements a row (arch: VectorThread rowMode).
        let row_mode = vector_memory && mode == 0 && !local(base_address);
        let row_elements = if row_mode {
            (16 * 8 / element_bits).min(self.lanes / self.memory_ports)
        } else {
            1
        };
        let step = if row_mode {
            self.memory_ports * row_elements
        } else if vector_memory {
            self.lanes.min(self.memory_ports)
        } else if fp_reduction || scalar_in || scalar_out {
            1
        } else {
            self.lanes
        };
        let mut cycles = waiting + 3;
        if permute {
            cycles += (8 * self.vector.vlen / self.vector.elen * 3 + 2) as u64;
        }
        let mut at = begin;
        let mut batch = 0;
        loop {
            let needed = [
                !vm,
                operand_needed,
                source_needed,
                destination_needed,
                reduction && batch == 0,
            ];
            // One read takes every operand word of the batch; a permute's source follows it.
            let mut words = Vec::new();
            for phase in 0..if batch == 0 { 5 } else { 4 } {
                if !needed[phase] {
                    continue;
                }
                let width = match phase {
                    0 => 1,
                    1 if mask_logical => 1,
                    1 if gather && op == 14 => 16,
                    1 => sew,
                    2 if mask_logical || population || first => 1,
                    2 if scalar_out || permute => sew,
                    2 => source_bits,
                    3 if vector_memory => element_bits,
                    3 if fp => sew,
                    3 => {
                        if wide {
                            sew * 2
                        } else {
                            sew
                        }
                    }
                    _ => sew,
                };
                if phase == 2 && permute {
                    let mut scratch = Vec::with_capacity(self.memory_ports.max(self.lanes));
                    for port in 0..self.memory_ports.max(self.lanes) {
                        let index = at + port;
                        let b = if vector_operand {
                            self.vector.read(rs1, index, if op == 14 { 16 } else { sew })
                        } else if kind == 3 {
                            rs1 as u64
                        } else {
                            self.x[rs1]
                        };
                        let offset = if slide_one { 1 } else { b as usize };
                        let source = if gather {
                            b as usize
                        } else if op == 14 {
                            index.wrapping_sub(offset)
                        } else {
                            index + offset
                        };
                        scratch.push(
                            source.wrapping_mul(width) / self.vector.elen % (8 * self.vector.vlen / self.vector.elen),
                        );
                    }
                    scratch.sort_unstable();
                    scratch.dedup();
                    cycles += (2 * scratch.len() + 3) as u64;
                } else {
                    let register = match phase {
                        0 => 0,
                        1 | 4 => rs1,
                        2 => rs2,
                        3 => ((insn >> 7) & 31) as usize,
                        _ => unreachable!(),
                    };
                    let ports = if row_mode {
                        step
                    } else if vector_memory {
                        self.memory_ports.min(self.lanes)
                    } else {
                        self.memory_ports.max(self.lanes)
                    };
                    let addresses = (0..ports)
                        .map(|port| {
                            let index = if phase == 4 || phase == 2 && scalar_out {
                                0
                            } else {
                                at + port
                            };
                            register * (self.vector.vlen / self.vector.elen) + index * width / self.vector.elen
                        })
                        .collect::<Vec<_>>();
                    words.extend(if phase == 4 { addresses[..1].to_vec() } else { addresses });
                }
            }
            if !words.is_empty() {
                // Operands takes a read after the writes of the last commit.
                cycles = cycles.max(self.vrf_ready.saturating_sub(self.cycles));
                cycles += self.operand_read_cycles(&words);
            }
            if at >= end && !scalar_in && !scalar_out {
                return cycles + 1;
            }
            let active = (at..(at + step).min(end)).filter(|&i| whole_memory || vm || self.vector.mask(0, i));
            let mut writes = !population && !first && !scalar_out;
            if vector_memory {
                writes = !store && active.clone().next().is_some();
                let mut groups = BTreeMap::<u64, usize>::new();
                let mut locals = 0;
                // A row is one bank access, two when it crosses a bank row.
                if row_mode {
                    let ports = (at..(at + step).min(end)).step_by(row_elements);
                    for first in ports {
                        let last = (first + row_elements).min(end) - 1;
                        if !(first..=last).any(|i| whole_memory || vm || self.vector.mask(0, i)) {
                            continue;
                        }
                        let start = base_address.wrapping_add((first * element_bits / 8) as u64);
                        let stop = base_address.wrapping_add(((last + 1) * element_bits / 8) as u64) - 1;
                        *groups.entry(start >> 32).or_default() += 1 + usize::from(start >> 4 != stop >> 4);
                    }
                }
                for index in active.clone().filter(|_| !row_mode) {
                    let offset = if indexed {
                        self.vector.read(rs2, index, memory_bits)
                    } else if mode == 2 {
                        (index as u64).wrapping_mul(self.x[rs2])
                    } else {
                        (index * element_bits / 8) as u64
                    };
                    let address = base_address.wrapping_add(offset);
                    if local(address) {
                        locals += 1;
                    } else {
                        *groups.entry(address >> 32).or_default() += 1;
                    }
                }
                let private = groups
                    .values()
                    .map(|&n| if store { 4 + 2 * n } else { 5 + n })
                    .max()
                    .unwrap_or(0);
                let local = if locals == 0 {
                    0
                } else if store {
                    locals + 1
                } else {
                    2 * locals + 1
                };
                cycles += private.max(local).max(1) as u64;
            } else {
                cycles += 1;
                if fp && (matches!(op, 32 | 33) || op == 19 && rs1 == 0) {
                    let mut extra = 0;
                    for index in active.clone() {
                        let a = self.vector.read(rs2, index, sew);
                        let b = if vector_operand {
                            self.vector.read(rs1, index, sew)
                        } else if sew == 32 && self.f[rs1] >> 32 != u32::MAX as u64 {
                            0x7fc0_0000
                        } else {
                            self.f[rs1]
                        };
                        extra = extra.max(if op == 33 {
                            floating_latency(b, a, sew, false)
                        } else {
                            floating_latency(a, b, sew, op == 19)
                        });
                    }
                    cycles += extra;
                }
                if integer && multiply && (32..=35).contains(&op) {
                    cycles += (sew + 2) as u64;
                }
                if reduction {
                    writes = at + step >= end;
                }
                if scalar_in {
                    writes = self.vector.vl != 0 && begin < self.vector.vl;
                }
            }
            // The commit: two cycles to hand the destination words of the batch to Operands, which writes them, a
            // bank a cycle each, behind the next batch or instruction (arch: VectorThread commit, Operands).
            if writes {
                let banks = self.lanes.min(self.vector.vlen / self.vector.elen);
                let bits = if vector_memory {
                    element_bits
                } else if wide {
                    sew * 2
                } else {
                    sew
                };
                let elements = (at..(at + step).min(end)).count().max(1);
                let words = (elements * bits).div_ceil(self.vector.elen);
                cycles += 2;
                self.vrf_ready = self.cycles + cycles + words.div_ceil(banks) as u64 + 3;
            }
            if scalar_in || scalar_out || at + step >= end {
                break;
            }
            at += step;
            batch += 1;
        }
        cycles
    }
}
