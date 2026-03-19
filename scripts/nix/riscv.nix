# RISC-V toolchain + riscv-pk as pure Nix derivations
{ pkgs }:

let
  riscvGcc = pkgs.pkgsCross.riscv64-embedded.buildPackages.gcc;
  riscvBinutils = pkgs.pkgsCross.riscv64-embedded.buildPackages.binutils;

  pkUrl = "https://github.com/riscv-software-src/riscv-pk.git";
  pkRev = "9c61d29846d8521d9487a57739330f9682d5b542";
  pkSrc = builtins.fetchGit { url = pkUrl; rev = pkRev; };
in
rec {
  pkDrv = pkgs.stdenv.mkDerivation {
    pname = "riscv-pk";
    version = pkRev;
    src = pkSrc;

    nativeBuildInputs = with pkgs; [
      gnumake
      autoconf
      automake
      libtool
      pkg-config
    ];

    buildInputs = [
      riscvGcc
      riscvBinutils
    ];

    dontConfigure = true;

    buildPhase = ''
      runHook preBuild
      mkdir -p build
      cd build

      export CC="${riscvGcc}/bin/riscv64-none-elf-gcc"
      export PATH="${riscvGcc}/bin:${riscvBinutils}/bin:$PATH"
      export OBJCOPY="${riscvBinutils}/bin/riscv64-none-elf-objcopy"
      export READELF="${riscvBinutils}/bin/riscv64-none-elf-readelf"
      host="$($CC -dumpmachine)"

      # riscv-pk expects cross compile host toolchain.
      ../configure --prefix="$out" --host="$host" ac_cv_prog_cc_cross=yes

      # GCC 13+ requires explicit zicsr/zifencei for csr/fence.i instructions.
      if [ -f Makefile ]; then
        sed -i 's/^\([[:space:]]*march := -march=\).*/\1rv64gc_zicsr_zifencei/' Makefile
        sed -i 's/^\([[:space:]]*mabi := -mabi=\).*/\1lp64/' Makefile
      fi

      make -j"$NIX_BUILD_CORES" march=-march=rv64gc_zicsr_zifencei mabi=-mabi=lp64
      make install

      # Make `pk` discoverable from devShell PATH.
      mkdir -p "$out/bin"
      pkPath="$(find "$out" -type f -path "*/bin/pk" | head -n 1 || true)"
      if [ -z "$pkPath" ]; then
        echo "ERROR: pk not found under $out after install"
        exit 1
      fi
      ln -sf "$pkPath" "$out/bin/pk"
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      true
    '';
  };

  buildInputs = [
    riscvGcc
    riscvBinutils
    pkDrv
  ];

  shellHook = ''
    echo "riscv gcc: $(command -v riscv64-none-elf-gcc)"
    echo "pk: $(command -v pk)"
  '';
}
