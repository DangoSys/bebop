{
  description = "Bebop - A buckyball emulator written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Rust toolchain
        rustToolchain = pkgs.rust-bin.stable.latest.default;

        # Host dependencies (for spike, ipc, etc.)
        hostDeps = with pkgs; [
          cmake
          ninja
          gcc
          boost
          dtc  # device tree compiler
        ];

        # Build the host (C/C++ part)
        bebopHost = pkgs.stdenv.mkDerivation {
          pname = "bebop-host";
          version = "0.1.0";

          src = ./host;

          nativeBuildInputs = [ pkgs.cmake pkgs.ninja ];
          buildInputs = hostDeps;

          configurePhase = ''
            mkdir -p build
            cd build
            cmake -G Ninja ..
          '';

          buildPhase = ''
            ninja -j$NIX_BUILD_CORES
          '';

          installPhase = ''
            mkdir -p $out
            cp -r ./* $out/
          '';
        };

        # Build the Rust emulator
        bebopRust = pkgs.rustPlatform.buildRustPackage {
          pname = "bebop";
          version = "0.1.0";

          src = ./bebop;

          cargoLock = {
            lockFile = ./bebop/Cargo.lock;
          };

          nativeBuildInputs = [ rustToolchain ];
          buildInputs = [ bebopHost ] ++ hostDeps;

          # Link to host libraries
          preBuild = ''
            export BEBOP_HOST_DIR=${bebopHost}
          '';

          # Install the binary
          installPhase = ''
            mkdir -p $out/bin
            cp target/release/bebop $out/bin/
          '';
        };

      in
      {
        packages = {
          host = bebopHost;
          emulator = bebopRust;
          default = bebopRust;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.cargo-watch
            pkgs.cargo-edit
          ] ++ hostDeps;

          shellHook = ''
            echo "Bebop development environment"
            echo "- Build host: cd host && mkdir -p build && cd build && cmake -G Ninja .. && ninja"
            echo "- Build emulator: cd bebop && cargo build --release --bin bebop"
            echo "- Run emulator: cd bebop && cargo run --release --bin bebop"
          '';
        };

        formatter = pkgs.nixpkgs-fmt;
      });
}
