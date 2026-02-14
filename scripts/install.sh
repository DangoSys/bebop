#!/bin/bash

set -e

BEBOP_DIR=$(git rev-parse --show-toplevel)

cd $BEBOP_DIR
git submodule update --init

$BEBOP_DIR/host/spike/install-spike.sh
# $BEBOP_DIR/host/gem5/install-gem5.sh
