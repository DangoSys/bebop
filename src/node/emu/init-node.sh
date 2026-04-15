#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$DIR/../../.." && pwd)"

cd "$REPO_ROOT"
cargo build --release --bin bebop

echo "[bemu] built: $REPO_ROOT/target/release/bebop"
