# bebop-next
A buckyball emulator written in Rust

### Quick start

1. Setup the repo

```
git clone https://github.com/DangoSys/bebop.git
```

2. Build the simulator

```
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
    --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# build bemu
cargo build --features bemu

# build p2e
cargo build --features p2e \
    --config="env.ARCH_CONFIG='sims.p2e.P2EToyConfig'"
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
cargo run --features p2e  -- p2e \
    --buildbitstream \
    --config="sims.p2e.P2EToyConfig"

cargo run --features p2e  -- p2e \
    --runworkload \
    --image="<image-file-path>" \
    --bitstream="<bitstream-file-path>"
```

<!-- cargo run --features verilator \
    --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'" \
    -- verilator \
    --elf="/home/wanghui/Code/buckyball/bb-tests/output/workloads/src/tutorial/tutorial-baremetal" \
    --log-dir="/home/wanghui/Code/buckyball/arch/log/test_log" \
    --fst-dir="/home/wanghui/Code/buckyball/arch/waveform/test_waveform"  -->
    