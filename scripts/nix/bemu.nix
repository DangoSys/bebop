{ pkgs }:

{
  # Spike is built from src/nodes/bemu/native/spike (vendored); these are build deps for configure/make.
  buildInputs = [
    pkgs.dtc
    pkgs.autoconf
    pkgs.automake
  ];

  shellHook = "";
}
