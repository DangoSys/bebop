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
# BEMU
cargo build --features bemu

# Verilator
cargo build --features verilator \
  --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# BEMU + Verilator
cargo build --features "bemu,verilator" \
  --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# P2E
cargo build --features p2e \
  --config="env.VSRC_PATH='<verilog-file-directory-path>'"
```

## Run

```bash
# BEMU
cargo run --features bemu -- bemu \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>"

# BEMU with proxy kernel
cargo run --features bemu -- bemu \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>" \
  --pk

# Verilator
cargo run --features verilator \
  --config="env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'" \
  -- verilator \
  --elf="<elf-file-path>" \
  --log-dir="<log-dir>" \
  --fst-dir="<fst-dir>"

# P2E build bitstream
cargo run --features p2e -- p2e \
  --buildbitstream \
  --build-dir="<design-build-dir>" \
  --output-dir="<bitstream-output-dir>"

# P2E run workload
cargo run --features p2e -- p2e \
  --runworkload \
  --image="<image-file-path>" \
  --bitstream="<bitstream-file-path>" \
  --log-dir="<log-dir>"
```

## Bank Hash

```bash
# Online difftest: BEMU -> background comparator + Verilator
cargo run --features "bemu,verilator" -- bank-hash-difftest \
  --elf="<elf-file-path>" \
  --out-dir="<out-dir>"

# Online difftest explicitly
cargo run --features "bemu,verilator" -- bank-hash-difftest \
  --elf="<elf-file-path>" \
  --out-dir="<out-dir>" \
  --compare-mode online

# Post difftest: BEMU -> Verilator -> comparator
cargo run --features "bemu,verilator" -- bank-hash-difftest \
  --elf="<elf-file-path>" \
  --out-dir="<out-dir>" \
  --compare-mode post

# Offline compare canonical logs
cargo run -- bank-hash-compare \
  --rtl="<out-dir>/rtl/log/rtl_bank_hash.canonical.ndjson" \
  --bemu="<out-dir>/bemu/bemu_bank_hash.canonical.ndjson" \
  --output="<out-dir>/bank_hash_compare.offline.ndjson"

# Replay packet stream compare
cargo run -- bank-hash-compare-stream \
  --input="<out-dir>/bank_hash_packets.ndjson" \
  --output="<out-dir>/bank_hash_compare.stream_replay.ndjson" \
  --idle-timeout-ms 100
```

## Bank Hash Outputs

```text
<out-dir>/bank_hash_packets.ndjson                  # BEMU/RTL shared runtime packet stream
<out-dir>/bank_hash_compare.ndjson                  # final compare result
<out-dir>/bemu/bemu_bank_hash.ndjson                # BEMU raw bank hash log
<out-dir>/bemu/bemu_bank_hash.canonical.ndjson      # BEMU normalized compare log
<out-dir>/rtl/log/rtl_bank_hash.ndjson              # RTL raw bank hash log
<out-dir>/rtl/log/rtl_bank_hash.canonical.ndjson    # RTL normalized compare log
```

## Tests

```bash
# Unit tests
cargo test

# Comparator tests
cargo test -q bank_hash_comparator

# Verilator crate tests
cargo test -q -p bebop-verilator

# BEMU tests
cargo nextest run --test test_bemu --features bemu

# Verilator tests
cargo nextest run --test test_verilator --features verilator \
  --config-file .config/nextest.toml \
  --config "env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# Bank Hash difftest tests
cargo nextest run --test test_bank_hash_difftest --features "bemu,verilator" \
  --config-file .config/nextest.toml \
  --config "env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# P2E tests
cargo nextest run --test test_p2e --features p2e -- \
  --p2e-bitstream "<bitstream-file-path>" \
  --p2e-build-dir "<design-build-dir>"
```
