{
  description = "bebop - A buckyball emulator written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [
          (import rust-overlay)
          (import ./scripts/nix/overlay.nix)
        ];
        pkgs = import nixpkgs { inherit system overlays; };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.base.autoconf
            pkgs.base.automake
            pkgs.base.libtool
            pkgs.base.gnumake
            pkgs.base.pkgConfig
            pkgs.base.rustAnalyzer
            pkgs.base.cargoWatch
            pkgs.base.preCommit
            pkgs.base.clangTools
            pkgs.base.cmake
            pkgs.base.ninja
            pkgs.base.dtc
            pkgs.base.gcc
            pkgs.base.boost
            pkgs.base.python3
            pkgs.base.rust

            pkgs.verilator
            pkgs.bebop
          ] ++ pkgs.riscv.buildInputs ++ pkgs.bemu.buildInputs;

          shellHook = pkgs.riscv.shellHook + ''
            pre-commit install --install-hooks --hook-type pre-commit -c tools/pre-commit-config.yaml
            echo "================= bebop development environment activated ========================="
            echo "Enable nodes including:"
            echo "bebop: $(command -v bebop)"
            echo "bemu: $(command -v bemu)"
            echo "verilator: $(command -v verilator)"
            echo "==========================================================================="
          '';
        };

        packages.default = pkgs.bebop;
      }
    );
}
