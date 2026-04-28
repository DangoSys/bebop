/// Generate a minimal device tree blob (DTB) for Linux kernel
///
/// This generates a very simple DTB with:
/// - Memory node
/// - CPU node
/// - Chosen node (for bootargs and initrd)

const FDT_BEGIN_NODE: u32 = 0x00000001;
const FDT_END_NODE: u32 = 0x00000002;
const FDT_PROP: u32 = 0x00000003;
const FDT_NOP: u32 = 0x00000004;
const FDT_END: u32 = 0x00000009;

pub struct DtbBuilder {
    data: Vec<u8>,
}

impl DtbBuilder {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn align(&mut self) {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
    }

    fn write_u32(&mut self, val: u32) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }

    fn write_u64(&mut self, val: u64) {
        self.data.extend_from_slice(&val.to_be_bytes());
    }

    fn write_string(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes());
        self.data.push(0); // null terminator
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
        self.write_u32(FDT_PROP);
        self.write_u32(value.len() as u32);
        self.write_u32(0); // nameoff (we'll fix this later)
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

    /// Build a minimal DTB for RISC-V Linux
    pub fn build_minimal(
        mem_base: u64,
        mem_size: u64,
        initrd_start: Option<u64>,
        initrd_end: Option<u64>,
    ) -> Vec<u8> {
        let mut builder = Self::new();

        // Root node
        builder.begin_node("");
        builder.property_u32("#address-cells", 2);
        builder.property_u32("#size-cells", 2);
        builder.property_string("compatible", "riscv-virtio");
        builder.property_string("model", "riscv-virtio,qemu");

        // Chosen node (for bootargs and initrd)
        builder.begin_node("chosen");
        builder.property_string("bootargs", "console=ttyS0 earlycon");
        if let (Some(start), Some(end)) = (initrd_start, initrd_end) {
            builder.property_u64("linux,initrd-start", start);
            builder.property_u64("linux,initrd-end", end);
        }
        builder.end_node();

        // Memory node
        builder.begin_node("memory@80000000");
        builder.property_string("device_type", "memory");
        let mut reg = Vec::new();
        reg.extend_from_slice(&mem_base.to_be_bytes());
        reg.extend_from_slice(&mem_size.to_be_bytes());
        builder.property("reg", &reg);
        builder.end_node();

        // CPUs node
        builder.begin_node("cpus");
        builder.property_u32("#address-cells", 1);
        builder.property_u32("#size-cells", 0);
        builder.property_u32("timebase-frequency", 10000000); // 10MHz

        // CPU 0
        builder.begin_node("cpu@0");
        builder.property_string("device_type", "cpu");
        builder.property_u32("reg", 0);
        builder.property_string("status", "okay");
        builder.property_string("compatible", "riscv");
        builder.property_string("riscv,isa", "rv64imafdcsu");
        builder.property_string("mmu-type", "riscv,sv39");

        // Interrupt controller
        builder.begin_node("interrupt-controller");
        builder.property_u32("#interrupt-cells", 1);
        builder.property_empty("interrupt-controller");
        builder.property_string("compatible", "riscv,cpu-intc");
        builder.end_node(); // interrupt-controller

        builder.end_node(); // cpu@0
        builder.end_node(); // cpus

        // SOC node
        builder.begin_node("soc");
        builder.property_u32("#address-cells", 2);
        builder.property_u32("#size-cells", 2);
        builder.property_string("compatible", "simple-bus");
        builder.property_empty("ranges");

        // UART node (SiFive UART at 0x10000000)
        builder.begin_node("serial@10000000");
        builder.property_string("compatible", "sifive,uart0");
        let mut uart_reg = Vec::new();
        uart_reg.extend_from_slice(&0x10000000_u64.to_be_bytes());
        uart_reg.extend_from_slice(&0x100_u64.to_be_bytes());
        builder.property("reg", &uart_reg);
        builder.property_u32("clock-frequency", 3686400); // Standard UART clock
        builder.end_node(); // serial@10000000

        builder.end_node(); // soc

        builder.end_node(); // root

        builder.write_u32(FDT_END);

        // Build final DTB with header
        let mut dtb = Vec::new();

        // FDT header
        dtb.extend_from_slice(&0xd00dfeed_u32.to_be_bytes()); // magic
        let total_size = 40 + builder.data.len(); // header + struct
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes()); // totalsize
        dtb.extend_from_slice(&40_u32.to_be_bytes()); // off_dt_struct
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes()); // off_dt_strings
        dtb.extend_from_slice(&(total_size as u32).to_be_bytes()); // off_mem_rsvmap
        dtb.extend_from_slice(&17_u32.to_be_bytes()); // version
        dtb.extend_from_slice(&16_u32.to_be_bytes()); // last_comp_version
        dtb.extend_from_slice(&0_u32.to_be_bytes()); // boot_cpuid_phys
        dtb.extend_from_slice(&0_u32.to_be_bytes()); // size_dt_strings
        dtb.extend_from_slice(&(builder.data.len() as u32).to_be_bytes()); // size_dt_struct

        // Struct data
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

        // Check magic number
        assert_eq!(&dtb[0..4], &[0xd0, 0x0d, 0xfe, 0xed]);

        // Check that it's not empty
        assert!(dtb.len() > 100);
    }
}
