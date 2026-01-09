#!/bin/bash

set -e

BEBOP_DIR=$(git rev-parse --show-toplevel)

cd $BEBOP_DIR
git submodule update --init

$BEBOP_DIR/host/spike/install-spike.sh
$BEBOP_DIR/host/gem5/install-gem5.sh

# cd $BEBOP_DIR/beboppy
# ln -s ${CONDA_PREFIX} ./python_modules || true
# npx motia create 

# cd $BEBOP_DIR/beboppy/steps && rm *.{py,json} || true
# cd $BEBOP_DIR/beboppy/steps && rm -r src/ || true
# cd $BEBOP_DIR/beboppy/steps && rm -r petstore/ || true
# cd $BEBOP_DIR/beboppy && rm -r src/ || true
# cd $BEBOP_DIR/beboppy && rm -r tutorial/ || true
# cd $BEBOP_DIR/beboppy && rm *.{md,tsx,rdb} || true
