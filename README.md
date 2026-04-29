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

cargo build --features verilator --config "env.ARCH_CONFIG='sims.verilator.BuckyballToyVerilatorConfig'"
```


