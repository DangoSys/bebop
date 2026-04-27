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
  clangTools = pkgs.clang-tools;
  cargo = pkgs.cargo;
  rustc = pkgs.rustc;
  rustfmt = pkgs.rustfmt;
  clippy = pkgs.clippy;
  preCommit = pkgs.pre-commit;

  riscvGcc = pkgs.pkgsCross.riscv64-embedded.buildPackages.gcc;
  riscvBinutils = pkgs.pkgsCross.riscv64-embedded.buildPackages.binutils;
}
