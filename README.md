# bebop
A buckyball emulator written in Rust


### Quick start

1. Activate the virtual environment
```
source $BUCKYBALL_PATH/env.sh
```

2. Build the simulator
```
./scripts/install.sh
```

3. Build the custom extension
```
cd customext
mkdir build && cd build
cmake ..
make
```

4. start the socket server
```
./scripts/bebop_setup.sh
```

5. run the program
```
spike --extension=toy --log-commits /home/mio/Code/buckyball/bb-tests/build/workloads/src/OpTest/gemmini/transpose-baremetal 2>/dev/null
```