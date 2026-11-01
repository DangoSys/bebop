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

## Build

```bash
bebop-bemu config --design toy --device ToyVerilatorConfig
bebop-bemu build --design toy --device ToyVerilatorConfig
```

## Run

Run an existing workload ELF:

```bash
bebop-bemu run --design toy --device ToyVerilatorConfig --elf /absolute/path/to/workload.elf
```
