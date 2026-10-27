use crate::thread::banks::{BankConfig, BankMap};
use crate::thread::instructions::context::{ExecContext, Instruction};
use crate::thread::instructions::decode::{rs1_b0, rs1_b1, rs1_b2, rs1_iter};
use crate::thread::kernels;

pub struct MvinKernel;
pub struct RunKernel;

struct Banks<'a, 'b> {
    ctx: &'a mut ExecContext<'b>,
    read_groups: u64,
}

impl Banks<'_, '_> {
    fn location(&self, address: u64) -> Result<(usize, usize, usize), kernels::MemoryError> {
        let bank = u32::try_from(address >> 32).map_err(|_| kernels::MemoryError)?;
        let offset = (address & 0xffff_ffff) as usize;
        if bank as usize > crate::configs::max_private_vbank() {
            return Err(kernels::MemoryError);
        }
        let config = self.ctx.cfgs.get(bank as usize).ok_or(kernels::MemoryError)?;
        if !config.allocated {
            return Err(kernels::MemoryError);
        }
        let columns = config.cols as usize;
        let line_bytes = crate::configs::bank_row_bytes();
        let group = (offset / line_bytes) % columns;
        let row = offset / (line_bytes * columns);
        let column_offset = offset % line_bytes;
        let physical = self
            .ctx
            .bank_map
            .resolve_group(bank, group as u32)
            .ok_or(kernels::MemoryError)?;
        let physical_offset = row * line_bytes + column_offset;
        if physical_offset >= self.ctx.banks[physical].len() {
            return Err(kernels::MemoryError);
        }
        Ok((physical, physical_offset, line_bytes - column_offset))
    }
}

impl kernels::Memory for Banks<'_, '_> {
    fn read(&mut self, address: u64, bytes: usize) -> Result<u64, kernels::MemoryError> {
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

    fn write(&mut self, address: u64, bytes: usize, value: u64) -> Result<(), kernels::MemoryError> {
        if address >> 32 < self.read_groups {
            return Err(kernels::MemoryError);
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

    fn ball_command(&mut self, funct7: u32, rs1: u64, rs2: u64) -> Result<u64, kernels::MemoryError> {
        let class = crate::configs::ball_domain::ball_class_for_funct(funct7).ok_or(kernels::MemoryError)?;
        let id = crate::configs::ball_domain::instance_for_funct(funct7).ok_or(kernels::MemoryError)?;
        // The GP domain ignores rs1[29] and rs1[63]: a kernel Ball command is not
        // renamed, so it updates its launch write bank in place, and it has no
        // old-destination read (wr_read_bank is 0).
        let rs1 = rs1 & !(1 << 29) | (1 << 63);
        let parent = self.ctx.ball_id.replace(id);
        let result = crate::ball::execute_known(&class, funct7, rs1, rs2, self.ctx);
        self.ctx.ball_id = parent;
        Ok(result)
    }
}

impl Instruction for MvinKernel {
    const FUNCT: u32 = 44;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bytes = usize::try_from(rs1_iter(xs1)).unwrap();
        let program = rs1_b2(xs1) as usize;
        let base = crate::configs::virtual_bank_num();
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
        let engine = ctx.kernel.as_mut().expect("thread has no RVV IP");
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
        let base = crate::configs::virtual_bank_num();
        assert!(program >= base && program < base + 2);
        assert!(xs1 >> 30 == 0 && read != write);
        assert!(read <= crate::configs::max_private_vbank() as u64);
        assert!(write <= crate::configs::max_private_vbank() as u64);
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
                configs[next] = BankConfig {
                    allocated: true,
                    cols: 1,
                    valid_rows: config.valid_rows,
                };
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
            *word = kernels::Memory::read(
                &mut Banks {
                    ctx,
                    read_groups: read_cfg.cols,
                },
                descriptor + index as u64 * 8,
                8,
            )
            .expect("RVV launch descriptor must be in write group zero");
        }
        assert!(words[11] == 0);
        let args: [u64; 8] = words[3..11].try_into().unwrap();
        let mut engine = ctx.kernel.take().expect("thread has no RVV IP");
        let result = engine.run(
            program - base,
            u32::try_from(words[0]).unwrap(),
            u32::try_from(words[1]).unwrap(),
            args,
            words[2],
            &mut Banks {
                ctx,
                read_groups: read_cfg.cols,
            },
        );
        *ctx.kernel = Some(engine);
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
