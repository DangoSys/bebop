final: prev:
{
  base = final.callPackage ./base.nix { };
  bemu = final.callPackage ./bemu.nix { };
  riscv = final.callPackage ./riscv.nix { };
  bebop = final.callPackage ./bebop.nix { };
  verilator = (import ./verilator.nix { pkgs = prev; }).verilator;
}
