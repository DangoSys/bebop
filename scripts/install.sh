#!/bin/bash

ROOT=$(dirname $0)/..


cd $ROOT/thirdparty/riscv-isa-sim
mkdir -p build
cd build
../configure --prefix=$RISCV --with-boost=no --with-boost-asio=no --with-boost-regex=no
make
make install
