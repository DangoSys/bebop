# bebop

Agile simulation framework for NPUs.

## Setup

```bash
git clone https://github.com/DangoSys/bebop.git
cd bebop
nix develop
```

## Build

```bash
# BEMU is an in-tree emulator backend. It does not need a separate
# simulator artifact build step.
cargo build --features bemu

# Verilator: build an RTL-bound runner executable.
cargo run --features verilator -- build verilator \
  --rtl-dir="<verilog-file-directory-path>" \
  --out-dir="<verilator-artifact-dir>"

# P2E: prepare a VVAC runtime case.
cargo run --features p2e -- build p2e \
  --rtl-dir="<verilog-file-directory-path>" \
  --out-dir="<p2e-case-dir>"
```

## Run

```bash
# BEMU
cargo run --features bemu -- run bemu \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>"

# BEMU with proxy kernel
cargo run --features bemu -- run bemu \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>" \
  --pk

# Verilator
cargo run --features verilator -- run verilator \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>" \
  --fst-dir="<fst-dir>"

# P2E run workload
cargo run --features p2e -- run p2e \
  --image="<image-file-path>" \
  --bitstream="<bitstream-file-path>" \
  --log-dir="<p2e-case-dir>"
```
