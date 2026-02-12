{ pkgs }:

pkgs.rustPlatform.buildRustPackage {
  pname = "bebop";
  version = "0.1.0";
  src = builtins.path { path = ../../bebop; name = "bebop-src"; };
  cargoLock.lockFile = ../../bebop/Cargo.lock;

  meta = with pkgs.lib; {
    description = "Bebop RISC-V emulator";
    license = licenses.asl20;
    platforms = platforms.linux;
  };
}
