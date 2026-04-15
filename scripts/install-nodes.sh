#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT/src/node/spike/init-node.sh"
"$ROOT/src/node/emu/init-node.sh"
"$ROOT/src/node/verilator/init-node.sh"

echo "[install] all nodes initialized"
