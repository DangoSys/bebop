#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cmake -S "$DIR" -B "$DIR/build"
cmake --build "$DIR/build" --target bebop_rocc

echo "[spike] built: $DIR/build/libbebop_rocc.so"
