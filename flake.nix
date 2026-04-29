{
  description = "bebop - A buckyball emulator written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # this old nixpkgs version has gcc8 which is needed for P2E vvac builds
    nixpkgs-gcc83 = {
      url = "github:NixOS/nixpkgs/nixos-19.03";
      flake = false;
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, nixpkgs-gcc83, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [
          (import ./scripts/nix/overlay.nix)
        ];
        pkgs = import nixpkgs { inherit system overlays; };
        gccPkgs = import nixpkgs-gcc83 { inherit system; };
        preCommitCfg = ./scripts/tools/pre-commit-config.yaml;
        preCommitInstall = pkgs.writeShellApplication {
          name = "bebop-pre-commit-install";
          runtimeInputs = [ pkgs.base.preCommit ];
          text = ''
            exec pre-commit install --install-hooks --hook-type pre-commit -c ${preCommitCfg}
          '';
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.base.autoconf
            pkgs.base.automake
            pkgs.base.libtool
            pkgs.base.gnumake
            pkgs.base.pkgConfig
            pkgs.base.clangTools
            pkgs.base.cmake
            pkgs.base.ninja
            pkgs.base.dtc
            pkgs.base.boost
            pkgs.base.python3
            pkgs.base.cargo
            pkgs.base.rustc
            pkgs.base.rustfmt
            pkgs.base.clippy
            pkgs.base.preCommit

            pkgs.verilator
            pkgs.bebop
            gccPkgs.gcc8
          ] ++ pkgs.riscv.buildInputs;

          shellHook = ''
            # Put gcc8 at the front of PATH for P2E vvac builds
            export PATH="${gccPkgs.gcc8}/bin:$PATH"
            hash -r
          '' + pkgs.riscv.shellHook + ''
            echo "================= bebop development environment activated ========================="
            echo "Enable nodes including:"
            echo "bebop: $(command -v bebop)"
            echo "riscv gcc: $(command -v riscv64-none-elf-gcc)"
            echo "verilator: $(command -v verilator)"
            echo "gcc for P2E: $(gcc --version | head -1)"
            echo "==========================================================================="
          '';
        };

        packages.default = pkgs.bebop;
        apps.pre-commit-install = {
          type = "app";
          program = "${preCommitInstall}/bin/bebop-pre-commit-install";
        };
      }
    );
}
