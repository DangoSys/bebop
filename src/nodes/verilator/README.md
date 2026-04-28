# Bebop Verilator - Rust Rewrite

This is a Rust rewrite of the Buckyball Verilator simulation framework, minimizing C++ code and maximizing Rust implementation.

## Architecture

```
Rust Layer (main business logic)
├── main.rs       - CLI entry and configuration
├── sim.rs        - Simulation control (init/step/exit)
├── trace.rs      - NDJSON trace logging (itrace/mtrace/pmctrace/ctrace)
├── dpi.rs        - DPI-C callback implementations
├── mmio.rs       - MMIO handling (sim exit, UART)
├── dram.rs       - DRAM simulation and ELF loading
└── ffi.rs        - FFI bindings to C++ layer

Minimal C++ Layer (Verilator interface only)
├── verilator.h   - C API declarations
└── verilator.cc  - Thin wrappers around Verilator C++ API
```

## Key Features

- **Minimal C++**: Only essential Verilator interface wrappers
- **Rust DPI-C**: All DPI-C callbacks implemented in Rust
- **No bdb debugger**: Batch mode only (interactive mode removed)
- **ELF loading**: Pure Rust using `goblin` crate
- **NDJSON traces**: All trace types (itrace/mtrace/pmctrace/ctrace/banktrace)

## Removed from csrc

- `bdb.cc` - Interactive debugger (readline-based)
- `monitor.cc` - Moved to Rust
- All `trace/*.cc` - DPI-C callbacks now in Rust
- `mmio.cc` - Moved to Rust
- `main.cc` - Moved to Rust

## Kept from csrc

- `BBSimDRAM.cc` - AXI4 memory model (DRAMSim2 integration)
- `mm*.cc` - Memory backend implementations

## Build

```bash
cd bebop/src/nodes/verilator
cargo build --release
```

## Usage

```bash
bebop-verilator <elf> --log <path> --fst <path> [--trace <items>] [--batch]
```

Options:
- `--log <path>`: NDJSON trace output
- `--fst <path>`: FST waveform output
- `--trace <items>`: Comma-separated list (all|none|itrace|mtrace|pmctrace|ctrace|banktrace)
- `--trace-mask <n>`: Bitfield (itrace=1 mtrace=2 pmctrace=4 ctrace=8 banktrace=16)
- `--batch`: Run in batch mode (default, interactive removed)

Environment:
- `BEBOP_VERILATOR_COVERAGE=true`: Enable coverage collection
