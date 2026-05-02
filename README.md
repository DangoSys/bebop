# bebop-next
A buckyball emulator written in Rust

### Quick start

1. Setup the repo

```
git clone https://github.com/DangoSys/bebop.git
cd bebop
git checkout next
```

2. Build the simulator

```
cd bebop
nix build
```

### Build

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
cargo run --features verilator -- verilator \
    --elf="<elf-file-path>"

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
