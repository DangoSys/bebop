use super::instruction::{ExecContext, Instruction};
use super::decode::{rs1_b0, rs1_b1, rs1_b2, rs1_iter};
use super::super::bank::{BankConfig, BankMap};

pub struct MvinKernel;
pub struct RunKernel;

struct Banks<'a, 'b> {
    ctx: &'a mut ExecContext<'b>,
    read_groups: u64,
}

impl Banks<'_, '_> {
    fn location(&self, address: u64) -> Result<(usize, usize, usize), rvv::MemoryError> {
        let bank = u32::try_from(address >> 32).map_err(|_| rvv::MemoryError)?;
        let offset = (address & 0xffff_ffff) as usize;
        if bank as usize > crate::config::private_vbank_upper_bound() {
            return Err(rvv::MemoryError);
        }
        let config = self.ctx.cfgs.get(bank as usize).ok_or(rvv::MemoryError)?;
        if !config.allocated {
            return Err(rvv::MemoryError);
        }
        let columns = config.cols as usize;
        let line_bytes = crate::config::bank_row_bytes();
        let group = (offset / line_bytes) % columns;
        let row = offset / (line_bytes * columns);
        let column_offset = offset % line_bytes;
        let physical = self.ctx.bank_map.resolve_group(bank, group as u32).ok_or(rvv::MemoryError)?;
        let physical_offset = row * line_bytes + column_offset;
        if physical_offset >= self.ctx.banks[physical].len() {
            return Err(rvv::MemoryError);
        }
        Ok((physical, physical_offset, line_bytes - column_offset))
    }
}

impl rvv::Memory for Banks<'_, '_> {
    fn read(&mut self, address: u64, bytes: usize) -> Result<u64, rvv::MemoryError> {
        let mut value = [0; 8];
        let mut done = 0;
        while done < bytes {
            let (bank, offset, available) = self.location(address + done as u64)?;
            let length = available.min(bytes - done);
            value[done..done + length].copy_from_slice(&self.ctx.banks[bank][offset..offset + length]);
            done += length;
        }
        Ok(u64::from_le_bytes(value))
    }

    fn write(&mut self, address: u64, bytes: usize, value: u64) -> Result<(), rvv::MemoryError> {
        if address >> 32 < self.read_groups {
            return Err(rvv::MemoryError);
        }
        let value = value.to_le_bytes();
        let mut done = 0;
        while done < bytes {
            let (bank, offset, available) = self.location(address + done as u64)?;
            let length = available.min(bytes - done);
            self.ctx.banks[bank][offset..offset + length].copy_from_slice(&value[done..done + length]);
            done += length;
        }
        Ok(())
    }

    fn ball_command(&mut self, funct7: u32, rs1: u64, rs2: u64) -> Result<u64, rvv::MemoryError> {
        let class = crate::config::ball_domain::ball_class_for_funct(funct7)
            .ok_or(rvv::MemoryError)?;
        Ok(crate::chip::execute_known(&class, funct7, rs1, rs2, self.ctx))
    }

}

impl Instruction for MvinKernel {
    const FUNCT: u32 = 44;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bytes = usize::try_from(rs1_iter(xs1)).unwrap();
        let program = rs1_b2(xs1) as usize;
        let base = crate::config::virtual_bank_num();
        assert!(program >= base && program < base + 2);
        let buffer = program - base;
        assert!(buffer < 2 && bytes >= 24 && bytes % 4 == 0);
        let mut image = vec![0; bytes];
        ctx.memory.read_buffer(xs2, &mut image);
        let header: Vec<u32> = image[..24]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect();
        assert_eq!(header[0], 0x31564b52, "invalid RVV kernel image magic");
        let text_bytes = header[1] as usize;
        let entry = header[2] as usize;
        let data_address = header[3];
        let data_bytes = header[4] as usize;
        let bss_bytes = header[5] as usize;
        assert!(text_bytes > 0 && text_bytes <= 4096 && text_bytes % 4 == 0);
        assert!(entry < text_bytes && entry % 4 == 0);
        assert!(data_bytes % 4 == 0);
        assert_eq!(bytes, 24 + text_bytes + data_bytes);
        assert_eq!(data_address, 0x40000000, "invalid RVV constant address");
        assert!(
            data_bytes <= 4096 && bss_bytes == 0,
            "RVV kernel globals must be readonly"
        );
        let engine = ctx.rvv.as_mut().expect("core has no RVV IP");
        engine.load_program(buffer, &image[24..24 + text_bytes]);
        engine.load_constants(buffer, &image[24 + text_bytes..]);
        0
    }
    fn latency(_xs1: u64, _xs2: u64) -> u64 {
        panic!("kernel latency requires the instruction-buffer and bank-access contract")
    }
}

impl Instruction for RunKernel {
    const FUNCT: u32 = 79;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let read = rs1_b0(xs1);
        let program = rs1_b1(xs1) as usize;
        let write = rs1_b2(xs1);
        let base = crate::config::virtual_bank_num();
        assert!(program >= base && program < base + 2);
        assert!(xs1 >> 30 == 0 && read != write);
        assert!(read <= crate::config::private_vbank_upper_bound() as u64);
        assert!(write <= crate::config::private_vbank_upper_bound() as u64);
        let read_cfg = *ctx.config(read);
        let write_cfg = *ctx.config(write);
        assert!(read_cfg.allocated && write_cfg.allocated);
        let total = (read_cfg.cols + write_cfg.cols) as usize;
        assert!(total <= ctx.cfgs.len());
        let read_only = (0..read_cfg.cols)
            .map(|group| ctx.bank_map.resolve_group(read as u32, group as u32).unwrap())
            .collect();
        let parent_read_only = std::mem::replace(&mut ctx.banks.read_only, read_only);
        let mut local = BankMap::new(ctx.bank_map.slots.len());
        let mut configs = vec![BankConfig::default(); ctx.cfgs.len()];
        let mut next = 0;
        for (parent, config) in [(read, read_cfg), (write, write_cfg)] {
            for group in 0..config.cols {
                let physical = ctx.bank_map.resolve_group(parent as u32, group as u32).unwrap();
                local.bind_group(physical, next as u32, 0);
                configs[next] = BankConfig { allocated: true, cols: 1, valid_rows: config.valid_rows };
                next += 1;
            }
        }
        let parent_map = std::mem::replace(ctx.bank_map, local);
        let parent_configs = ctx.cfgs.to_vec();
        ctx.cfgs.copy_from_slice(&configs);
        let descriptor = (read_cfg.cols << 32) | xs2;
        assert!(xs2 <= u32::MAX as u64);
        let mut words = [0u64; 12];
        for (index, word) in words.iter_mut().enumerate() {
            *word = rvv::Memory::read(&mut Banks { ctx, read_groups: read_cfg.cols }, descriptor + index as u64 * 8, 8)
                .expect("RVV launch descriptor must be in write group zero");
        }
        assert!(words[11] == 0);
        let args: [u64; 8] = words[3..11].try_into().unwrap();
        let mut engine = ctx.rvv.take().expect("core has no RVV IP");
        let result = engine.run(program - base, u32::try_from(words[0]).unwrap(),
                                u32::try_from(words[1]).unwrap(), args, words[2], &mut Banks { ctx, read_groups: read_cfg.cols });
        *ctx.rvv = Some(engine);
        ctx.banks.read_only = parent_read_only;
        *ctx.bank_map = parent_map;
        ctx.cfgs.copy_from_slice(&parent_configs);
        result.unwrap_or_else(|fault| panic!("RVV kernel fault: {fault:?}"));
        write
    }

    fn latency(_xs1: u64, _xs2: u64) -> u64 {
        panic!("kernel latency requires the instruction-buffer and bank-access contract")
    }
}
