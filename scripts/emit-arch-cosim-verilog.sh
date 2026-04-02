#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# -lt 1 ]]; then
  echo "usage: emit-arch-cosim-verilog.sh <out-dir>" >&2
  exit 1
fi
OUT="$1"
if [[ -z "${BEBOP_ARCH_ROOT:-}" ]]; then
  echo "BEBOP_ARCH_ROOT is not set" >&2
  exit 1
fi
ARCH="$BEBOP_ARCH_ROOT"
if [[ ! -d "$ARCH" ]]; then
  echo "arch repo not found at $ARCH (BEBOP_ARCH_ROOT)" >&2
  exit 1
fi
JOBS=""
if [[ -n "${BEBOP_MILL_JOBS:-}" ]]; then
  JOBS="$BEBOP_MILL_JOBS"
elif [[ -n "${NIX_BUILD_CORES:-}" ]] && [[ "$NIX_BUILD_CORES" != "0" ]]; then
  JOBS="$NIX_BUILD_CORES"
else
  JOBS=16
fi
if [[ ! "$JOBS" =~ ^[0-9]+$ ]] || [[ "$JOBS" -le 0 ]]; then
  echo "invalid BEBOP_MILL_JOBS/NIX_BUILD_CORES: $JOBS" >&2
  exit 1
fi
command -v mill >/dev/null 2>&1 || { echo "mill not in PATH" >&2; exit 1; }
mkdir -p "$OUT"
cd "$ARCH"
mill --jobs "$JOBS" buckyball.runMain sims.bebop.EmitBebopSpikeCosimVerilog "$(realpath "$OUT")"
echo "Emitted Chisel Verilog into $OUT"
