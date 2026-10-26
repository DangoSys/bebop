# bebop

Agile simulation framework for NPUs.

## Setup

```bash
git clone https://github.com/DangoSys/bebop.git
cd bebop
nix develop
```

Run configuration and build commands from the Bebop checkout root. Bebop initializes
`thirdparty/buckyball` and its `bbdev` submodule, then executes the configuration
installation scripts directly. Generated design configuration and build manifests
stay under `thirdparty/buckyball/examples/designs/<design>/`.

## Build

```bash
# Install one design configuration without building the simulator.
cargo run --bin bebop -- config --design pebble

# Build BEMU using the Buckyball design configuration and Ball implementations.
cargo run --bin bebop -- build bemu \
  --design-dir="/absolute/path/to/buckyball/examples/designs/pebble"

# Verilator: build an RTL-bound runner executable.
cargo run --bin bebop -- build verilator \
  --design-dir="/absolute/path/to/buckyball/examples/designs/pebble" \
  --rtl-dir="<verilog-file-directory-path>"

# P2E: prepare a VVAC runtime case.
cargo run --features p2e --bin bebop -- build p2e \
  --design-dir="/absolute/path/to/buckyball/examples/designs/pebble" \
  --rtl-dir="<verilog-file-directory-path>" \
  --out-dir="<p2e-case-dir>"
```

## Run

```bash
# BEMU
/absolute/path/to/buckyball/examples/designs/pebble/output/bin/bebop-bemu \
  --elf="<elf-file-path>" \
  --log-dir="/absolute/path/to/log"

# Verilator
/absolute/path/to/buckyball/examples/designs/pebble/output/bin/bebop-verilator \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>"

# P2E run workload
/absolute/path/to/buckyball/examples/designs/pebble/output/bin/bebop-p2e \
  --image="<image-file-path>" \
  --bitstream="<bitstream-file-path>" \
  --log-dir="<p2e-case-dir>"
```

Verilator waveforms (when enabled) go to `<log-dir>/waveform/waveform.fst`.
