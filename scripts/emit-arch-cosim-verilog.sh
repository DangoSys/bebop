#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -lt 1 ]]; then
  echo "usage: emit-arch-cosim-verilog.sh <out-dir> [arch-root]" >&2
  exit 1
fi
OUT="$1"
if [[ -n "${2:-}" ]]; then
  ARCH="$2"
else
  ARCH="$ROOT/../arch"
fi
if [[ ! -d "$ARCH" ]]; then
  echo "arch repo not found at $ARCH" >&2
  exit 1
fi
JOBS=16
command -v mill >/dev/null 2>&1 || { echo "mill not in PATH" >&2; exit 1; }
mkdir -p "$OUT"
cd "$ARCH"
mill --jobs "$JOBS" buckyball.runMain sims.bebop.EmitBebopSpikeCosimVerilog "$(realpath "$OUT")"
echo "Emitted Chisel Verilog into $OUT"
