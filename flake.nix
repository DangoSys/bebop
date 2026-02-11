{
  description = "Bebop emulator, host IPC, Spike and gem5 toolchain";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import ./scripts/nix/overlay.nix) ];
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
            pkgs.rustc
            pkgs.cargo
            pkgs.pkg-config
          ];
          shellHook = ''
            echo "Bebop development shell"
            echo " - build Rust CLI: cargo build --release --bin bebop"
            echo " - host libs available under ${pkgs.bebopHost}"
            echo " - spike binary available under ${pkgs.bebopSpike}"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}