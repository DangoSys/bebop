{ pkgs }:

let
  spikeCommit = "dbb30bb31a086decd0cf1376dca3418d0a9bac64";
  spikeRepo = "https://github.com/riscv-software-src/riscv-isa-sim.git";
in
{
  buildInputs = [
    pkgs.dtc
    pkgs.autoconf
    pkgs.automake
    pkgs.git
  ];

  shellHook = ''
    # Clone spike directly to the target path if not present
    SPIKE_TARGET="src/nodes/bemu/native/spike"
    if [ ! -d "$SPIKE_TARGET/.git" ] || [ ! -f "$SPIKE_TARGET/configure.ac" ]; then
      echo "Cloning spike to $SPIKE_TARGET..."
      rm -rf "$SPIKE_TARGET"
      mkdir -p "$(dirname "$SPIKE_TARGET")"
      git clone ${spikeRepo} "$SPIKE_TARGET"
      (cd "$SPIKE_TARGET" && git checkout ${spikeCommit})
      echo "Spike ready at $SPIKE_TARGET (commit: ${spikeCommit})"
    fi
  '';
}
