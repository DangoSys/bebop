#!/usr/bin/env bash
# Build libbemu.so and Spike (with bebop_rocc). Run from repo root.
set -e
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "Building libbemu.so ..."
cargo build --release
LIBBEMU="$ROOT/target/release/libbemu.so"
if [[ ! -f "$LIBBEMU" ]]; then
  echo "ERROR: $LIBBEMU not found"
  exit 1
fi
echo "Built: $LIBBEMU"

SPIKE_SRC="$ROOT/thirdparty/spike"
SPIKE_BUILD="${SPIKE_BUILD:-$SPIKE_SRC/build}"
SPIKE_PREFIX="${SPIKE_PREFIX:-$SPIKE_BUILD/install}"
mkdir -p "$SPIKE_BUILD"
cd "$SPIKE_BUILD"
if [[ ! -f Makefile ]]; then
  echo "Configuring Spike ..."
  "$SPIKE_SRC/configure" --prefix="$SPIKE_PREFIX"
fi
echo "Building Spike (with bebop_rocc in customext) ..."
make -j"${NPROC:-$(nproc)}"
echo "Spike built. Install with: make -C $SPIKE_BUILD install"
echo "Then run tests with: LD_LIBRARY_PATH=$ROOT/target/release:\$LD_LIBRARY_PATH $SPIKE_PREFIX/bin/spike --extension=bebop_rocc pk <elf>"
