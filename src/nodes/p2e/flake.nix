{
  description = "P2E Control Path Build Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs-gcc83 = {
      url = "github:NixOS/nixpkgs/nixos-19.03";
      flake = false;
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, nixpkgs-gcc83, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        gccPkgs = import nixpkgs-gcc83 { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          name = "p2e-control-path-build";

          buildInputs = with pkgs; [
            cmake
            gnumake
          ] ++ [
            gccPkgs.gcc8
          ];

          shellHook = ''
            export PATH="${gccPkgs.gcc8}/bin:$PATH"

            # Create a dummy clang-format that does nothing
            # This prevents vvac from using the system clang-format which has libstdc++ conflicts
            mkdir -p "$PWD/.dummy_bin"
            cat > "$PWD/.dummy_bin/clang-format" << 'DUMMY_EOF'
#!/bin/bash
# Dummy clang-format that does nothing to avoid libstdc++ conflicts
exit 0
DUMMY_EOF
            chmod +x "$PWD/.dummy_bin/clang-format"
            export PATH="$PWD/.dummy_bin:$PATH"

            hash -r
            echo "P2E Control Path Build Environment"
            echo "CMake version: $(cmake --version | head -1)"
            echo "GCC version: $(gcc --version | head -1)"
            if [ "$(gcc -dumpfullversion)" != "8.3.0" ]; then
              echo "ERROR: expected gcc 8.3.0"
              exit 1
            fi

            # Source HPEC environment
            if [ -f "$PWD/sourceme.sh" ]; then
              source "$PWD/sourceme.sh"
            fi
          '';
        };
      }
    );
}
