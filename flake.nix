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
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        wasmEnv = import ./scripts/nix/wasm.nix { inherit pkgs rustToolchain; };
        tauriEnv = import ./scripts/nix/tauri.nix { inherit pkgs rustToolchain; };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.cargo-watch
            self.packages.${system}.default
          ];

          shellHook = ''
            echo "bebop dev environment ready"
            echo "  cargo build    - build the project"
            echo "  cargo run -- batch  - print hello world"
            echo "  cargo run -- -h     - show help"
          '';
        };

        devShells.wasm = pkgs.mkShell {
          buildInputs = wasmEnv.buildInputs;
          shellHook = wasmEnv.shellHook;
        };

        devShells.tauri = pkgs.mkShell {
          buildInputs = tauriEnv.buildInputs;
          shellHook = tauriEnv.shellHook;
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "bebop";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          # Only build the CLI binary; tauri/wasm members need extra system libs
          cargoBuildFlags = [ "--package" "bebop" ];
        };
      }
    );
}
