# bebop

Agile simulation framework for NPUs.

## Setup

Standalone BEMU requires Nix and a Bebop checkout:

```bash
git clone https://github.com/DangoSys/bebop.git
cd bebop
nix run .#install
nix develop .#bemu
```

Installation uses the pinned `thirdparty/buckyball` revision and binds its Cargo
paths to this Bebop checkout. Reinstalling updates that dependency while preserving
unrelated local edits; conflicting edits are reported by Git. In library mode,
Bebop uses the enclosing Buckyball checkout instead.

Executing an existing ELF does not require a stack checkout. Workloads and Linux
images are built separately by stack.

## Build

```bash
bebop-bemu config --config toy/devices/toy/configs/ToyDefaultConfig.toml --device toy
bebop-bemu build --config toy/devices/toy/configs/ToyDefaultConfig.toml --device toy
```

## Run

Run an existing workload ELF:

```bash
bebop-bemu run --config toy/devices/toy/configs/ToyDefaultConfig.toml --device toy --elf /absolute/path/to/workload.elf
```

Pass model-runner options after `--`, for example:

```bash
bebop-bemu run --config toy/devices/toy/configs/ToyDefaultConfig.toml --device toy --elf /path/to/firmware.elf \
  -- --ddr-image /path/to/ddr.img --tool-profile
```
