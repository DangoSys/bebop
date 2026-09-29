{ pkgs }:

{
  buildInputs = [
    pkgs.dtc
    pkgs.protobuf
    pkgs.git
  ];

  shellHook = ''
    if [ ! -e src/nodes/bemu/rvcpu ]; then
      git clone https://github.com/DangoSys/riscv-cpu-sim src/nodes/bemu/rvcpu || exit 1
    fi
  '';
}
