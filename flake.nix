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
          cargoBuildFlags = [ "--package" "bebop" ];
          nativeBuildInputs = with pkgs; [ verilator python3 ];
          buildInputs = [ spikeEnv.spikeDrv ] ++ riscvEnv.buildInputs;
        };

        # Copy bebop into the same $out as libbebop_rocc.so. `symlinkJoin` leaves
        # bin/bebop pointing into bebopCli; on Linux /proc/self/exe resolves to that
        # store path, so path_rocc_so's ../lib misses the rocc derivation.
        bebopPkg = pkgs.runCommand "bebop-with-rocc" { } ''
          mkdir -p "$out/bin" "$out/lib" "$out/share/bebop"
          cp -r "${bebopCli}/bin/." "$out/bin/"
          cp "${spikeEnv.bebopRoccDrv}/lib/libbebop_rocc.so" "$out/lib/"
          cp "${./src/node/emu/configs/config.toml}" "$out/share/bebop/config.toml"
          chmod +x "$out/bin/"*
        '';
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
            pre-commit install --install-hooks --hook-type pre-commit -c tools/pre-commit-config.yaml
            echo "bebop: $(command -v bebop)"
            echo "spike: $(command -v spike)"
            echo "pk: $(command -v pk)"
          '';
        };

        packages.default = bebopPkg;

        # Expose spike derivation to allow `nix build .#spike` verification.
        packages.spike = spikeEnv.spikeDrv;
        packages.rocc = spikeEnv.bebopRoccDrv;
        packages.pk = riscvEnv.pkDrv;
      }
    );
}
