{ pkgs }:

{
  autoconf = pkgs.autoconf;
  automake = pkgs.automake;
  libtool = pkgs.libtool;
  gnumake = pkgs.gnumake;
  pkgConfig = pkgs.pkg-config;
  cmake = pkgs.cmake;
  ninja = pkgs.ninja;
  dtc = pkgs.dtc;
  gcc = pkgs.gcc;
  boost = pkgs.boost.dev;
  python3 = pkgs.python3;
  rustAnalyzer = pkgs.rust-analyzer;
  cargoWatch = pkgs.cargo-watch;
  preCommit = pkgs.pre-commit;
  clangTools = pkgs.clang-tools;
  rust = pkgs.rust-bin.stable.latest.default;

  riscvGcc = pkgs.pkgsCross.riscv64-embedded.buildPackages.gcc;
  riscvBinutils = pkgs.pkgsCross.riscv64-embedded.buildPackages.binutils;
}
