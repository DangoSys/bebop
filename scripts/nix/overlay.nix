final: prev:
{
  base = final.callPackage ./base.nix { };
  riscv = final.callPackage ./riscv.nix { };
  bebop = final.callPackage ./bebop.nix { };
  bemu = final.callPackage ./bemu.nix { };
  verilator = (import ./verilator.nix { pkgs = prev; }).verilator;
  p2e = final.callPackage ./p2e.nix { pkgs = prev; };
}
