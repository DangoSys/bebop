use crate::constants::*;

#[derive(Default)]
pub struct DtbBuilder {
    data: Vec<u8>,
}

impl DtbBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn align(&mut self) {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
    }

    fn write_u32(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }

    fn write_string(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0);
        self.align();
    }

    fn begin_node(&mut self, name: &str) {
        self.write_u32(FDT_BEGIN_NODE);
        self.write_string(name);
    }

    fn end_node(&mut self) {
        self.write_u32(FDT_END_NODE);
    }

    fn property(&mut self, _name: &str, value: &[u8]) {
        self.write_u32(FDT_PROP);
        self.write_u32(value.len() as u32);
        self.write_u32(0);
        self.data.extend_from_slice(value);
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
        builder.property_string("bootargs", "console=ttyS0 earlycon");
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
        builder.end_node();

        builder.end_node();
        builder.end_node();

        builder.begin_node("soc");
        builder.property_u32("#address-cells", 2);
        builder.property_u32("#size-cells", 2);
        builder.property_string("compatible", "simple-bus");
        builder.property_empty("ranges");

        builder.begin_node("serial@10000000");
        builder.property_string("compatible", "sifive,uart0");
        let mut uart_reg = Vec::new();
        uart_reg.extend_from_slice(&0x10000000_u64.to_be_bytes());
        uart_reg.extend_from_slice(&0x100_u64.to_be_bytes());
        builder.property("reg", &uart_reg);
        builder.property_u32("clock-frequency", 3686400);
        builder.end_node();

        builder.end_node();

        builder.end_node();

        builder.write_u32(FDT_END);

        let mut dtb = Vec::new();

        dtb.extend_from_slice(&0xd00dfeed_u32.to_be_bytes());
        let total_size = 40 + builder.data.len();
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes());
        dtb.extend_from_slice(&40_u32.to_be_bytes());
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes());
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes());
        dtb.extend_from_slice(&17_u32.to_be_bytes());
        dtb.extend_from_slice(&16_u32.to_be_bytes());
        dtb.extend_from_slice(&0_u32.to_be_bytes());
        dtb.extend_from_slice(&0_u32.to_be_bytes());
        dtb.extend_from_slice(&(builder.data.len() as u32).to_be_bytes());

        dtb.extend_from_slice(&builder.data);

        dtb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_minimal_dtb() {
        let dtb = DtbBuilder::build_minimal(0x80000000, 1 << 30, None, None);
        assert_eq!(&dtb[0..4], &[0xd0, 0x0d, 0xfe, 0xed]);
        assert!(dtb.len() > 100);
    }
}
