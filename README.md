# bebop
A buckyball emulator written in Rust


### Quick start

1. Build the repo
```
./scripts/install.sh
```

1. Build the simulator
```
cd bebop
cargo build --release --bin bebop 
```

1. Run the simulation
```
cd bebop
./target/release/bebop
```

run in quiet with only workload logs
```
cargo run --release --bin bebop -- -q
```
