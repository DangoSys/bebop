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


# run bemu (baremetal mode, starts in M-mode)
cargo run --features bemu -- bemu \
    --elf="<elf-file-path>" \
    --log-dir="<log-file-directory-path>"

# run bemu (Linux mode with proxy kernel, starts in S-mode)
cargo run --features bemu -- bemu \
    --elf="<elf-file-path>" \
    --log-dir="<log-file-directory-path>" \
    --pk

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

### Bank Hash Logs

BEMU and RTL keep their raw observation logs:

```
bemu_bank_hash.ndjson
rtl_bank_hash.ndjson
```

They also emit normalized logs for comparison:

```
bemu_bank_hash.canonical.ndjson
rtl_bank_hash.canonical.ndjson
```

Raw `instruction_id` values are not directly comparable across backends: BEMU
uses a software NPU instruction counter, while RTL currently records the ROB id.
Bank hash comparison should use canonical records, filtering
`event_class == "bank_data_write"` and matching only by `comparable_seq`,
`bank_id`, and `version`. RTL boot/init events such as `pc == 0` and
config/control/memory-only instructions are preserved or classified in canonical
logs but are not part of the default bank data comparison. Current canonical
classes cover the registered BEMU NPU funct7 values: `control_only` for fence,
barrier, flush, and counter ops; `config_only` for config/allocation ops;
`memory_only` for non-Bank-SRAM memory or MMIO writes; and `bank_data_write` for
ops that write main Bank SRAM.

For the normal one-command Bank Hash difftest, run both backends and the stream
comparator through one CLI entry point:

```
cargo run --features "bemu,verilator" -- bank-hash-difftest \
    --elf="<elf-file-path>" \
    --out-dir="<bank-hash-difftest-output-dir>"
```

If `VSRC_PATH` is not set, the Verilator build tries to infer
`arch/build/${ARCH_CONFIG}` from an ancestor of the repository, defaulting
`ARCH_CONFIG` to `sims.verilator.BuckyballToyVerilatorConfig`. Set `VSRC_PATH`
explicitly when using a nonstandard Verilator build directory.

This creates:

```
<out-dir>/bemu/bemu_bank_hash.ndjson
<out-dir>/bemu/bemu_bank_hash.canonical.ndjson
<out-dir>/rtl/log/rtl_bank_hash.ndjson
<out-dir>/rtl/log/rtl_bank_hash.canonical.ndjson
<out-dir>/bank_hash_packets.ndjson
<out-dir>/bank_hash_compare.ndjson
```

The command exits with success only when every comparable
`bank_data_write` packet matches and at least one comparable packet was checked.
It exits non-zero on `MISMATCH`, `MISSING_RTL`, `MISSING_BEMU`, or an
inconclusive zero-compare run. If BEMU or RTL exits with a functional failure,
the command still runs the Bank Hash comparator over the packets that were
generated before exit, writes `bank_hash_compare.ndjson`, and then returns
non-zero with both the backend failure and compare summary available in logs.

For debugging or offline fallback, the lower-level three-step flow is still
available. Pass the same packet stream path to both runs and start the stream
comparator on that file:

```
cargo run --features bemu -- bemu \
    --elf="<elf-file-path>" \
    --log-dir="<bemu-log-dir>" \
    --bank-hash-stream="<shared-bank-hash-packets.ndjson>"

cargo run --features verilator -- verilator \
    --elf="<elf-file-path>" \
    --log-dir="<rtl-log-dir>" \
    --fst-dir="<rtl-fst-dir>" \
    --no-wave \
    --bank-hash-stream="<shared-bank-hash-packets.ndjson>"

cargo run -- bank-hash-compare-stream \
    --input="<shared-bank-hash-packets.ndjson>" \
    --output="<bank_hash_compare.ndjson>"
```

The stream path is append-only so independent RTL and BEMU processes can submit
canonical packets to the same file. The runtime comparator consumes already
generated packets and writes `bank_hash_compare.ndjson`; it does not read RTL
Bank arrays. RTL Bank Hash generation still happens synchronously at the stable
checkpoint before packet submission. If stream comparison is not enabled, the
raw and canonical NDJSON logs remain sufficient for offline comparison with
`bank-hash-compare`.

# Batch Test

```
# run bemu tests with buckyball 
cargo nextest run --test test_bemu --features bemu

# run verilator tests with buckyball 
cargo nextest run --test test_verilator --features verilator \
  --config-file .config/nextest.toml \
  --config "env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# run bank hash difftest tests with buckyball
cargo nextest run --test test_bank_hash_difftest --features "bemu,verilator" \
  --config-file .config/nextest.toml \
  --config "env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"

# run p2e tests with buckyball
cargo nextest run --test test_p2e --features p2e \
  -- \
  --p2e-bitstream "<bitstream-file-path>" \
  --p2e-build-dir "<design-build-directory-path>"
```

The Bank Hash difftest batch harness runs the one-command
`bank-hash-difftest` flow per workload. Its default discovery set is
`OpTest/buckyball/*singlecore-baremetal`; each test fails if either backend
fails functionally or if the final Bank Hash compare reports `MISMATCH`,
`MISSING_RTL`, `MISSING_BEMU`, or zero comparable packets.
