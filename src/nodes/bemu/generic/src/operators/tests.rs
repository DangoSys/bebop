use super::{Engine, Memory, MemoryError};

fn engine(fp64: bool) -> Engine {
    let mut config = crate::configs::GpDomainConfig::default();
    config.lane_number = 4;
    config.v_len = 128;
    config.e_len = 64;
    config.i_buf_words = 64;
    config.memory_ports = 4;
    config.enable = Some(true);
    config.fp64 = Some(fp64);
    Engine::new(&config, 16384, None)
}

struct TestMemory {
    value: u64,
    reads: usize,
    writes: usize,
}

impl Memory for TestMemory {
    fn read(&mut self, address: u64, _bytes: usize) -> Result<u64, MemoryError> {
        assert_eq!(address, 0);
        self.reads += 1;
        Ok(self.value)
    }

    fn write(&mut self, address: u64, _bytes: usize, value: u64) -> Result<(), MemoryError> {
        assert_eq!(address, 0);
        self.writes += 1;
        self.value = value;
        Ok(())
    }

    fn ball_command(&mut self, _funct7: u32, _rs1: u64, _rs2: u64, _elapsed: u64) -> Result<(u64, u64), MemoryError> {
        panic!("Unexpected Ball command")
    }
}

#[test]
fn fp32_policy_keeps_integer64_and_single_conversions() {
    let mut e = engine(false);
    e.f[1] = 0xffffffff3f800000;
    e.f[2] = 0xffffffff40000000;
    e.instruction = (2 << 20) | (1 << 15) | (3 << 7) | 0x53; // fadd.s
    e.scalar_float().unwrap();
    assert_eq!(e.f[3], 0xffffffff40400000);
    assert_eq!(e.fcsr, 0);

    e.x[1] = 1 << 40;
    e.instruction = (0x68 << 25) | (2 << 20) | (1 << 15) | (3 << 7) | 0x53; // fcvt.s.l
    e.scalar_float().unwrap();
    assert_eq!(e.f[3], 0xffffffff53800000);
    e.instruction = (0x60 << 25) | (2 << 20) | (2 << 15) | (3 << 7) | 0x53; // fcvt.l.s
    e.scalar_float().unwrap();
    assert_eq!(e.x[3], 2);

    let mut memory = TestMemory {
        value: 0x123456789abcdef0,
        reads: 0,
        writes: 0,
    };
    e.x[1] = 0;
    e.instruction = (1 << 15) | (3 << 12) | (3 << 7) | 0x03; // ld
    e.scalar_execute(&mut memory).unwrap();
    assert_eq!(e.x[3], memory.value);
    e.x[2] = 0xfedcba9876543210;
    e.instruction = (2 << 20) | (1 << 15) | (3 << 12) | 0x23; // sd
    e.scalar_execute(&mut memory).unwrap();
    assert_eq!(memory.value, e.x[2]);
}

#[test]
fn fp32_policy_rejects_double_arithmetic_moves_conversion_and_memory() {
    let mut e = engine(false);
    for instruction in [
        (1 << 25) | 0x53,                // fadd.d
        (9 << 25) | 0x53,                // fmul.d
        (0x0d << 25) | 0x53,             // fdiv.d
        (0x2d << 25) | 0x53,             // fsqrt.d
        (0x71 << 25) | 0x53,             // fmv.x.d
        (0x79 << 25) | 0x53,             // fmv.d.x
        (0x20 << 25) | (1 << 20) | 0x53, // fcvt.s.d
        (0x21 << 25) | 0x53,             // fcvt.d.s
        (1 << 25) | 0x43,                // fmadd.d
    ] {
        e.instruction = instruction;
        assert_eq!(e.scalar_float().unwrap_err().cause, 2, "instruction={instruction:08x}");
    }
    let mut memory = TestMemory {
        value: 0,
        reads: 0,
        writes: 0,
    };
    for instruction in [(3 << 12) | 0x07, (3 << 12) | 0x27] {
        // fld/fsd
        e.instruction = instruction;
        assert_eq!(e.scalar_execute(&mut memory).unwrap_err().cause, 2);
    }
    assert_eq!((memory.reads, memory.writes), (0, 0));
}

#[test]
fn fp64_policy_preserves_double_arithmetic_and_loads() {
    let mut e = engine(true);
    e.f[1] = 1.0f64.to_bits();
    e.f[2] = 2.0f64.to_bits();
    e.instruction = (1 << 25) | (2 << 20) | (1 << 15) | (3 << 7) | 0x53;
    e.scalar_float().unwrap();
    assert_eq!(e.f[3], 3.0f64.to_bits());
    let mut memory = TestMemory {
        value: 4.0f64.to_bits(),
        reads: 0,
        writes: 0,
    };
    e.instruction = (3 << 12) | (3 << 7) | 0x07;
    e.scalar_execute(&mut memory).unwrap();
    assert_eq!(e.f[3], memory.value);
}

#[test]
#[should_panic(expected = "explicit GP FP64 policy")]
fn missing_fp64_policy_is_an_error() {
    let mut config = crate::configs::GpDomainConfig::default();
    config.lane_number = 4;
    config.v_len = 128;
    config.e_len = 64;
    config.i_buf_words = 64;
    Engine::new(&config, 16384, None);
}
