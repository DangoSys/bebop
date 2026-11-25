mod tests {
    use bebop_bemu_ilp::Rocket;
    use generic::timing::CpuTiming;
    use mlp::DirectMapped;

    #[test]
    fn rocket_streams_hits_and_pays_redirects_and_load_use() {
        let mut timing = Rocket::new(Box::new(DirectMapped::new(64, 40)), Box::new(DirectMapped::new(64, 40)));
        for (pc, instruction, cycles) in [
            // The first instruction misses the I$; the next ones in its line stream.
            (0x1000, 0x13, 1 + 3 + 40),
            (0x1004, 0x1, 1),
            // ld x10, 0(x11) misses the D$, then a use of x10.
            (0x1006, 0x0005_b503, 1 + 40),
            (0x100a, 0x0005_0593, 2),
            // A taken jump to a compressed instruction in the same line.
            (0x1020, 0x1, 4),
            // A redirect to a 4-byte instruction in the upper halfword.
            (0x1032, 0x13, 5),
        ] {
            let decoded = if instruction & 3 == 3 {
                instruction
            } else {
                generic::riscv::expand_compressed(instruction as u16).unwrap()
            };
            assert_eq!(timing.instruction(pc, instruction, decoded, 0), cycles);
        }
    }
}
