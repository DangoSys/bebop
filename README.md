# bebop-next
An agile simulation framework for NPUs.

Currently support: emulator (bemu), verilator, FPGA (P2E)

### Quick start


```
git clone https://github.com/DangoSys/bebop.git
cd bebop
nix build
```



### Build

<!-- CARGO_TARGET_DIR=target-xxx  -->
```
cd bebop
nix develop

# build verilator
cargo build --features verilator \
    --config="env.VSRC_PATH='<verilog-file-directory-path>'" \
    --config="env.OUT_PATH='<generate-file-directory-path>'"

# build bemu
cargo build --features bemu

# build p2e
cargo build --features p2e \
    --config="env.VSRC_PATH='<verilog-file-directory-path>'"
```


### Run

```
cd bebop
nix develop

# run verilator
cargo run --features verilator \
    --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'" \
    -- verilator \
    --elf="<elf-file-path>" \
    --log-dir="<log-file-directory-path>" \
    --fst-dir="<fst-file-directory-path>" 


# run bemu
cargo run --features bemu -- bemu \
    --elf="<elf-file-path>"

# run p2e
cargo run --features p2e -- p2e \
    --buildbitstream \
    --build-dir="<design-build-directory-path>" \
    --output-dir="<bitstream-file-directory-path>"

cargo run --features p2e  -- p2e \
    --runworkload \
    --image="<image-file-path>" \
    --bitstream="<bitstream-file-path>"
    --log-dir="<log-file-directory-path>"
```

# run elf_bemu
cargo nextest run --test elf_bemu --features bemu

# run elf_verilator
cargo nextest run --test elf_verilator --features verilator \
  --config-file .config/nextest.toml \
  --config "env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

<!-- cargo run --features verilator \
    --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'" \
    -- verilator \
    --elf="/home/wanghui/Code/buckyball/bb-tests/output/workloads/src/tutorial/tutorial-baremetal" \
    --log-dir="/home/wanghui/Code/buckyball/arch/log/test_log" \
    --fst-dir="/home/wanghui/Code/buckyball/arch/waveform/test_waveform"  -->
    