#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCH="${BEBOP_ARCH_ROOT:-$ROOT/../arch}"
OUT="${1:-$ROOT/src/verilator/gen}"
if [[ ! -d "$ARCH" ]]; then
  echo "arch repo not found at $ARCH; set BEBOP_ARCH_ROOT" >&2
  exit 1
fi
command -v mill >/dev/null 2>&1 || { echo "mill not in PATH" >&2; exit 1; }
mkdir -p "$OUT"
cd "$ARCH"
mill buckyball.runMain sims.bebop.EmitBebopSpikeCosimVerilog "$(realpath "$OUT")"
echo "Emitted Chisel Verilog into $OUT"
