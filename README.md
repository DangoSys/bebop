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

### Run

```
cd bebop
nix develop
bebop bemu /path/to/pk-tests
bebop bemu /path/to/pk-tests --step   # per allocated bank 64-bit hash after each RoCC insn
bebop verilator /path/to/pk-tests      # verilator-engine only, RTL SHM lane (Unix + `verilator`)
bebop difftest /path/to/pk-tests       # bemu-tests + verilator-engine, dual lane + optional FNV `bank_digest` check
bebop bemu /path/to/pk-tests --step --all-banks   # optional: print every bank (default: allocated only)
```

