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
      in
      {
        packages = {
          bebop = pkgs.bebop;
          host = pkgs.bebopHost;
          spike = pkgs.Spike;
          gem5 = pkgs.Gem5;
          default = pkgs.bebop;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.bebop
            pkgs.Spike
            pkgs.Gem5
            pkgs.rustc
            pkgs.cargo
            pkgs.pkg-config
          ];
          shellHook = ''
            echo "Bebop development shell"
            echo " - bebop path: $(which bebop)"
            echo " - spike path: $(which spike)"
            echo " - gem5.opt path: $(which gem5.opt)"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
