{
  description = "Bebop emulator, host IPC, Spike and gem5 toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    spike-src = {
      url = "github:riscv-software-src/riscv-isa-sim/45fe6c110aed80d5689752236ba0a668f093ce48";
      flake = false;
    };
    gem5-src = {
      url = "github:gem5/gem5/ddd4ae35adb0a3df1f1ba11e9a973a5c2f8c2944";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, spike-src, gem5-src }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import ./scripts/nix/overlay.nix { inherit spike-src gem5-src; }) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        bebopCli = pkgs.rustPlatform.buildRustPackage {
          pname = "bebop-cli";
          version = "0.1.0";
          src = builtins.path { path = ./bebop; name = "bebop-cli-src"; };
          cargoLock.lockFile = ./bebop/Cargo.lock;
        };
      in
      {
        packages = {
          bebop = bebopCli;
          host = pkgs.bebopHost;
          spike = pkgs.bebopSpike;
          gem5 = pkgs.bebopGem5;
          default = bebopCli;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            bebopCli
            pkgs.bebopSpike
            pkgs.bebopGem5
            pkgs.rustc
            pkgs.cargo
            pkgs.pkg-config
          ];
          shellHook = ''
            echo "Bebop development shell"
            echo " - bebop, spike, gem5.opt are available in PATH"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
