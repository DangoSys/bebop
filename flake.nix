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
        spikeEnv = import ./scripts/nix/spike.nix {
          inherit pkgs;
          bebopSrc = ./.;
        };
        riscvEnv = import ./scripts/nix/riscv.nix { inherit pkgs; };

        bebopCli = pkgs.rustPlatform.buildRustPackage {
          pname = "bebop";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          # Only build the CLI binary; tauri/wasm members need extra system libs
          cargoBuildFlags = [ "--package" "bebop" ];
          nativeBuildInputs = with pkgs; [ verilator python3 ];
          buildInputs = [ spikeEnv.spikeDrv ] ++ riscvEnv.buildInputs;
        };

        bebopPkg = pkgs.symlinkJoin {
          name = "bebop-with-rocc";
          paths = [ bebopCli spikeEnv.bebopRoccDrv ];
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postBuild = ''
            wrapProgram $out/bin/bebop \
              --set BEBOP_ROCC_SO ${spikeEnv.bebopRoccDrv}/lib/libbebop_rocc.so
          '';
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.cargo-watch
            pkgs.pre-commit
            pkgs.clang-tools
            pkgs.cmake
            pkgs.ninja
            pkgs.verilator
            bebopPkg
          ] ++ spikeEnv.buildInputs ++ riscvEnv.buildInputs;

          shellHook = riscvEnv.shellHook + ''
            export BEBOP_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
            pre-commit install --install-hooks --hook-type pre-commit -c tools/pre-commit-config.yaml
            echo "bebop: $(command -v bebop)"
            echo "spike: $(command -v spike)"
            echo "pk: $(command -v pk)"
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

        packages.default = bebopPkg;

        # Expose spike derivation to allow `nix build .#spike` verification.
        packages.spike = spikeEnv.spikeDrv;
        packages.rocc = spikeEnv.bebopRoccDrv;
        packages.pk = riscvEnv.pkDrv;
      }
    );
}
