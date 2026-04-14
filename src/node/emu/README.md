# BEMU and Spike Integration

This directory integrates **BEMU** (Bebop Emulator, the golden model for Buckyball custom instructions) with **Spike** (RISC-V ISA simulator).  
When guest code executes **custom-0** (opcode `0x0b`), Spike communicates with an independent BEMU process via RoCC extension `bebop_rocc` over **POSIX shared memory** RPC, instead of `dlopen`ing `libbemu.so` inside Spike.

## Architecture Overview

- **BEMU** (`src/emu`): Rust golden model running in the **`bebop worker-shm`** child process (parallel to Spike), directly calling `Bemu::execute` / `write_memory` / `read_memory`.
- **Shared-memory layout** (must match C++): [`src/spike/bebop_shm.h`](../spike/bebop_shm.h) and [`src/shm/layout.rs`](../shm/layout.rs). **`BEBOP_SHM_SIZE` = 8192**. Each lane has `req` / `ack` + `bebop_msg_t` (including **`bank_digest`** for difftest). Cosim uses four lanes: **`cmd_bemu` / `cmd_rtl` / `mem_bemu` / `mem_rtl`**. **`bebop bemu`** uses only **`cmd_bemu` + `mem_bemu`**.
- **`bebop_rocc`** ([`src/spike/bebop_rocc.cc`](../spike/bebop_rocc.cc) -> `libbebop_rocc.so`): on custom-0 path, it `mmap`s segment **`BEBOP_SHM_NAME`**, synchronizes with worker via `req`/`ack`; **MVIN** still reads blocks from Spike MMU and writes to BEMU with **`OP_SYNC`**; **MVOUT** uses **`OP_READ`** then writes back to MMU; regular instructions use **`OP_HANDLE`**.
- **`bebop bemu`**: Spike + **`bemu-tests`** only (BEMU golden only).
- **Node protocol** ([`src/node/node.rs`](../node/node.rs)): `bebop` main process is node0; `runner` allocates monotonically increasing `node_id` from **`--node-file`** for Spike and sidecars. **`bemu-tests`** and **`verilator-engine`** (Unix cosim) each call `alloc_node_id`.
- **`bebop verilator`**: Spike + **`verilator-engine` only**; Spike starts with **`--extlib=<absolute path to libbebop_rocc.so>` + `--extension=bebop_rocc`**; uses only **`cmd_rtl` + `mem_rtl`** (no `bemu-tests`, no BEMU-side `b0=` step line).
- **`bebop difftest`**: Spike + **`bemu-tests` + `verilator-engine`**; dual cmd/mem run in parallel and **`rd` must match**. With **`BEBOP_DIFFTEST=1`**, Spike also checks dual-lane **`bank_digest`** (same FNV rule as BEMU `cosim_aggregate_banks_digest`). `bebop_cosim_banks` decodes `funct` only at **`issue_start`**; **`banks_busy`** (for example multi-cycle `mul64`) is merged into `rtl_busy` to avoid premature sampling. **`bebop bemu`** only uses **`cmd_bemu` + `mem_bemu`**. In **`--step`** mode, BEMU `b0=...` and FNV digest remain different metrics. Add **`--all-banks`** to print all banks. Cosim requires **Unix**.

Custom instructions are RISC-V custom-0; `funct7` / `rs1` / `rs2` map to BEMU `funct`, `xs1`, `xs2`. MVIN/MVOUT use guest virtual addresses; BEMU internal addressing is modulo 512KB and stays semantically aligned with Spike after synchronization.

## End-to-End Flow (run in order)

```bash
nix develop
cargo build --release
./target/release/bebop bemu /path/to/your-test-linux
./target/release/bebop verilator /path/to/your-test-linux
./target/release/bebop difftest /path/to/your-test-linux

./target/release/bebop bemu /path/to/ctest_vecunit_tiled_matmul-linux --step
./target/release/bebop verilator /path/to/ctest_vecunit_tiled_matmul-linux --step
./target/release/bebop difftest /path/to/ctest_vecunit_tiled_matmul-linux --step

./target/release/bebop bemu /path/to/ctest_vecunit_matmul_random1-linux --step
./target/release/bebop verilator /path/to/ctest_vecunit_matmul_random1-linux --step
./target/release/bebop difftest /path/to/ctest_vecunit_matmul_random1-linux --step
```

- **`cargo build --release`** builds bebop CLI, `libbemu.so`, etc. `build.rs` auto-sets Verilator `make` parallelism (`BEBOP_MAKE_JOBS` first, then `NIX_BUILD_CORES`, default `16`) and keeps `vl_bebop` for incremental builds.
- To force a clean full Verilator rebuild: `BEBOP_CLEAN_VL=1 cargo build --release`.
- **`cmake` / `ninja`** builds **`src/spike/build/libbebop_rocc.so`** under **`src/spike`** (CMake must `find_program(spike)`).
- **`bebop bemu <ELF>`** / **`bebop verilator <ELF>`** require full path of built RISC-V Linux test binary. Missing **`libbebop_rocc.so`** fails fast. Lookup order: **`src/spike/build/libbebop_rocc.so`** (relative to `target/{debug,release}/bebop`) -> **`../lib/libbebop_rocc.so`** (install/Nix). No extra `BEBOP_ROCC_SO` runtime config needed.
- **IPC timing summary**: `bebop bemu` / `verilator` / `difftest` print summaries to **stderr** by default: one for **Spike** (custom0 split in `bebop_rocc`) and one for each **Rust worker**. `bebop` sets child **`BEBOP_IPC_STATS`** to `1` or `0` (`--no-ipc-stats` => `0`). If Spike loads `libbebop_rocc.so` standalone and this env var is unset, `bebop_rocc` also prints Spike summary by default; set **`BEBOP_IPC_STATS=0`** to disable. After editing [`bebop_rocc.cc`](../spike/bebop_rocc.cc), rebuild with `cmake --build src/spike/build --target bebop_rocc`.

## Configuration

BEMU config sources (**environment variables are not used**):

- **Explicit**: global option on any subcommand: **`bebop --config /path/to/config.toml ...`** (forwarded to `bemu-tests` workers).
- **Default** (when `--config` is not provided), in order:
  1. Prefix-relative path: **`../share/bebop/config.toml`** (for example Nix `bebop-with-rocc`)
  2. If source tree still exists locally: **`src/emu/configs/config.toml`** (relative to compile-time crate root)

If none is available, or parsing fails, the process exits with an explicit error.

## File Map

| Path | Description |
|-----|------|
| `src/emu/` | BEMU (Rust), [`runner.rs`](runner.rs) (`bemu-tests` RPC), [`vl_engine.rs`](vl_engine.rs) (`verilator-engine`, Unix) |
| `src/emu/interface/capi_exports.rs` | C API (still available for other hosts via `dlopen`) |
| `src/shm/` | POSIX shm layout aligned with `bebop_shm.h` |
| `src/spike/` | `bebop_rocc.cc`, `bebop_shm.h`, `CMakeLists.txt`, `runner.rs` |
