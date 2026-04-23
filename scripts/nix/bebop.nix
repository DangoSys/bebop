{ pkgs, base, verilator, bebopSrc ? ../.., bemu, riscv }:

let
  bebopCli = pkgs.rustPlatform.buildRustPackage {
    pname = "bebop";
    version = "0.1.0";
    src = bebopSrc;
    cargoLock.lockFile = "${bebopSrc}/Cargo.lock";
    cargoBuildFlags = [ "--package" "bebop" ];
    nativeBuildInputs = [
      verilator
      base.python3
    ];
    buildInputs = [ bemu.spikeDrv ] ++ riscv.buildInputs;
  };
in
pkgs.runCommand "bebop-with-rocc" { } ''
  mkdir -p "$out/bin" "$out/lib" "$out/share/bebop"
  cp -r "${bebopCli}/bin/." "$out/bin/"
  cp "${bemu.bebopRoccDrv}/lib/libbebop_rocc.so" "$out/lib/"
  cp "${bebopSrc}/src/node/emu/configs/config.toml" "$out/share/bebop/config.toml"
  chmod +x "$out/bin/"*
''
