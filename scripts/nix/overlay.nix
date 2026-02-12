{ spike-src, gem5-src }:

final: prev:

let
  bebop = final.callPackage ./bebop-install.nix { };
  bebopHost = final.callPackage ./bebop-lib-install.nix { };
  spike = final.callPackage ./spike-install.nix {
    inherit bebopHost;
    spikeSrc = spike-src;
  };
  gem5 = final.callPackage ./gem5-install.nix {
    inherit bebopHost;
    gem5Src = gem5-src;
  };
in
{
  bebop = bebop;
  bebopHost = bebopHost;
  bebopSpike = spike;
  bebopGem5 = gem5;
}
