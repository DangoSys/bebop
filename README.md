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

# current focus: retain the verilator path only
bebop verilator /path/to/pk-tests
```

`bemu` and `p2e` are intentionally not wired at this stage.
