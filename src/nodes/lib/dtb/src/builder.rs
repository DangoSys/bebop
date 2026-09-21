use crate::constants::*;

#[derive(Default)]
pub struct DtbBuilder {
    structure: Vec<u8>,
    strings: Vec<u8>,
}

impl DtbBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn align(&mut self) {
        while !self.structure.len().is_multiple_of(4) {
            self.structure.push(0);
        }
    }

    fn write_u32(&mut self, val: u32) {
        self.structure.extend_from_slice(&val.to_be_bytes());
    }

    fn write_string(&mut self, s: &str) {
        self.structure.extend_from_slice(s.as_bytes());
        self.structure.push(0);
        self.align();
    }

    fn begin_node(&mut self, name: &str) {
        self.write_u32(FDT_BEGIN_NODE);
        self.write_string(name);
    }

    fn end_node(&mut self) {
        self.write_u32(FDT_END_NODE);
    }

    fn property(&mut self, name: &str, value: &[u8]) {
        let name_offset = self.strings.len();
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);

        self.write_u32(FDT_PROP);
        self.write_u32(value.len() as u32);
        self.write_u32(name_offset as u32);
        self.structure.extend_from_slice(value);
        self.align();
    }

    fn property_u32(&mut self, name: &str, value: u32) {
        self.property(name, &value.to_be_bytes());
    }

    fn property_u64(&mut self, name: &str, value: u64) {
        self.property(name, &value.to_be_bytes());
    }

    fn property_string(&mut self, name: &str, value: &str) {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        self.property(name, &bytes);
    }

    fn property_empty(&mut self, name: &str) {
        self.property(name, &[]);
    }

    pub fn build_minimal(mem_base: u64, mem_size: u64, initrd_start: Option<u64>, initrd_end: Option<u64>) -> Vec<u8> {
        let mut builder = Self::new();

        builder.begin_node("");
        builder.property_u32("#address-cells", 2);
        builder.property_u32("#size-cells", 2);
        builder.property_string("compatible", "riscv-virtio");
        builder.property_string("model", "riscv-virtio,qemu");

        builder.begin_node("chosen");
        builder.property_string("bootargs", "console=ttySIF0 earlycon");
        builder.property_string("stdout-path", "/soc/serial@60020000");
        if let (Some(start), Some(end)) = (initrd_start, initrd_end) {
            builder.property_u64("linux,initrd-start", start);
            builder.property_u64("linux,initrd-end", end);
        }
        builder.end_node();

        builder.begin_node("memory@80000000");
        builder.property_string("device_type", "memory");
        let mut reg = Vec::new();
        reg.extend_from_slice(&mem_base.to_be_bytes());
        reg.extend_from_slice(&mem_size.to_be_bytes());
        builder.property("reg", &reg);
        builder.end_node();

        builder.begin_node("cpus");
        builder.property_u32("#address-cells", 1);
        builder.property_u32("#size-cells", 0);
        builder.property_u32("timebase-frequency", 10000000);

        builder.begin_node("cpu@0");
        builder.property_string("device_type", "cpu");
        builder.property_u32("reg", 0);
        builder.property_string("status", "okay");
        builder.property_string("compatible", "riscv");
        builder.property_string("riscv,isa", "rv64imafdcsu");
        builder.property_string("mmu-type", "riscv,sv39");

        builder.begin_node("interrupt-controller");
        builder.property_u32("#interrupt-cells", 1);
        builder.property_empty("interrupt-controller");
        builder.property_string("compatible", "riscv,cpu-intc");
        builder.property_u32("phandle", 1);
        builder.end_node();

        builder.end_node();
        builder.end_node();

        builder.begin_node("soc");
        builder.property_u32("#address-cells", 2);
        builder.property_u32("#size-cells", 2);
        builder.property_string("compatible", "simple-bus");
        builder.property_empty("ranges");

        builder.begin_node("clint@2000000");
        builder.property_string("compatible", "riscv,clint0");
        let mut clint_reg = Vec::new();
        clint_reg.extend_from_slice(&bebop_clint::BASE.to_be_bytes());
        clint_reg.extend_from_slice(&bebop_clint::SIZE.to_be_bytes());
        builder.property("reg", &clint_reg);
        let mut clint_interrupts = Vec::new();
        for cell in [1_u32, 3, 1, 7] {
            clint_interrupts.extend_from_slice(&cell.to_be_bytes());
        }
        builder.property("interrupts-extended", &clint_interrupts);
        builder.end_node();

        builder.begin_node("plic@c000000");
        builder.property_string("compatible", "riscv,plic0");
        builder.property_empty("interrupt-controller");
        builder.property_u32("#interrupt-cells", 1);
        builder.property_u32("riscv,ndev", 1);
        builder.property_u32("phandle", 2);
        let mut plic_reg = Vec::new();
        plic_reg.extend_from_slice(&bebop_plic::BASE.to_be_bytes());
        plic_reg.extend_from_slice(&bebop_plic::SIZE.to_be_bytes());
        builder.property("reg", &plic_reg);
        let mut plic_interrupts = Vec::new();
        for cell in [1_u32, 11, 1, 9] {
            plic_interrupts.extend_from_slice(&cell.to_be_bytes());
        }
        builder.property("interrupts-extended", &plic_interrupts);
        builder.end_node();

        builder.begin_node("serial@60020000");
        builder.property_string("compatible", "sifive,uart0");
        let mut uart_reg = Vec::new();
        uart_reg.extend_from_slice(&0x60020000_u64.to_be_bytes());
        uart_reg.extend_from_slice(&0x100_u64.to_be_bytes());
        builder.property("reg", &uart_reg);
        builder.property_u32("clock-frequency", 3686400);
        builder.end_node();

        builder.end_node();

        builder.end_node();

        builder.write_u32(FDT_END);

        const HEADER_SIZE: usize = 40;
        const RESERVE_MAP_SIZE: usize = 16;
        let structure_offset = HEADER_SIZE + RESERVE_MAP_SIZE;
        let strings_offset = structure_offset + builder.structure.len();
        let total_size = strings_offset + builder.strings.len();

        let mut dtb = Vec::with_capacity(total_size);
        dtb.extend_from_slice(&0xd00dfeed_u32.to_be_bytes());
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes());
        dtb.extend_from_slice(&(structure_offset as u32).to_be_bytes());
        dtb.extend_from_slice(&(strings_offset as u32).to_be_bytes());
        dtb.extend_from_slice(&(HEADER_SIZE as u32).to_be_bytes());
        dtb.extend_from_slice(&17_u32.to_be_bytes());
        dtb.extend_from_slice(&16_u32.to_be_bytes());
        dtb.extend_from_slice(&0_u32.to_be_bytes());
        dtb.extend_from_slice(&(builder.strings.len() as u32).to_be_bytes());
        dtb.extend_from_slice(&(builder.structure.len() as u32).to_be_bytes());

        dtb.extend_from_slice(&[0; RESERVE_MAP_SIZE]);
        dtb.extend_from_slice(&builder.structure);
        dtb.extend_from_slice(&builder.strings);

        dtb
    }
}
