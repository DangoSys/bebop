#!/usr/bin/env bash
# clang-format via nix (same binary for devshell and IDE git).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/.." && pwd)"
style="file:${root}/tools/clang-format.yaml"

exec nix shell --extra-experimental-features "nix-command flakes" nixpkgs#clang-tools -c \
  clang-format -i --style="$style" -- "$@"
