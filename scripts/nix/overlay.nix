{ spike-src, gem5-src }:

final: prev:

let
  bebopHost = final.callPackage ./bebop-install.nix { };
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
  bebopHost = bebopHost;
  bebopSpike = spike;
  bebopGem5 = gem5;
}
