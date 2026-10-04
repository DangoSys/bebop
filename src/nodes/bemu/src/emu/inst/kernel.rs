use super::instruction::{ExecContext, Instruction};

pub struct MvinKernel;
pub struct RunKernel;

struct Banks<'a, 'b>(&'a mut ExecContext<'b>);

impl Banks<'_, '_> {
    fn location(&self, address: u32, bytes: usize) -> Result<(usize, usize), rvv::MemoryError> {
        let bank = address >> 16;
        let offset = (address & 0xffff) as usize;
        if bank as usize > crate::config::private_vbank_upper_bound() {
            return Err(rvv::MemoryError);
        }
        let config = self.0.cfgs.get(bank as usize).ok_or(rvv::MemoryError)?;
        if !config.allocated || config.cols != 1 {
            return Err(rvv::MemoryError);
        }
        let physical = self.0.bank_map.resolve_group(bank, 0).ok_or(rvv::MemoryError)? as usize;
        if offset + bytes > self.0.banks[physical].len() {
            return Err(rvv::MemoryError);
        }
        Ok((physical, offset))
    }
}

impl rvv::Memory for Banks<'_, '_> {
    fn read(&mut self, address: u32, bytes: usize) -> Result<u64, rvv::MemoryError> {
        let (bank, offset) = self.location(address, bytes)?;
        let mut value = [0; 8];
        value[..bytes].copy_from_slice(&self.0.banks[bank][offset..offset + bytes]);
        Ok(u64::from_le_bytes(value))
    }

    fn write(&mut self, address: u32, bytes: usize, value: u64) -> Result<(), rvv::MemoryError> {
        let (bank, offset) = self.location(address, bytes)?;
        self.0.banks[bank][offset..offset + bytes].copy_from_slice(&value.to_le_bytes()[..bytes]);
        Ok(())
    }
}

impl Instruction for MvinKernel {
    const FUNCT: u32 = 12;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bytes = xs1 as u32 as usize;
        let buffer = (xs1 >> 32) as usize;
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
        assert_eq!(data_address, 0x80000000, "invalid RVV constant address");
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
    const FUNCT: u32 = 15;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        assert!(xs1 <= u32::MAX as u64 && xs2 < 2);
        let mut words = [0u32; 12];
        for (index, word) in words.iter_mut().enumerate() {
            *word = rvv::Memory::read(&mut Banks(ctx), xs1 as u32 + index as u32 * 4, 4)
                .expect("RVV launch descriptor must be in an allocated bank") as u32;
        }
        assert!(words[11] == 0);
        let args: [u32; 8] = words[3..11].try_into().unwrap();
        let mut engine = ctx.rvv.take().expect("core has no RVV IP");
        let result = engine.run(xs2 as usize, words[0], words[1], args, words[2], &mut Banks(ctx));
        *ctx.rvv = Some(engine);
        result.unwrap_or_else(|fault| panic!("RVV kernel fault: {fault:?}"));
        (args[0] >> 16) as u64
    }
    fn latency(_xs1: u64, _xs2: u64) -> u64 {
        panic!("kernel latency requires the instruction-buffer and bank-access contract")
    }
}
