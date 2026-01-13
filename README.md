# bebop
A buckyball emulator written in Rust


### Quick start

<!-- 1. Activate the virtual environment
```
source $BUCKYBALL_PATH/env.sh
``` -->

1. Build the repo
```
./scripts/install.sh
```

2. Build the simulator
```
cd bebop
cargo build --release --bin bebop 
```

3. Run the simulation
```
cd bebop
./target/release/bebop
```

run in quiet with only workload logs
```
cargo run --release --bin bebop -- -q
```
