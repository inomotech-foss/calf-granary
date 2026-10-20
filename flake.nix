{
  description = "Build caches for CI runners";

  inputs = {
    cornucopia = {
      url = "github:inomotech-foss/cornucopia/336224d4815abb555987607a940bbd3769f67f89";
      inputs.nixpkgs.follows = "ephemeris/nixpkgs";
    };
    ephemeris.url = "git+https://codeberg.org/inomotech/ephemeris";
  };

  outputs =
    inputs@{ ephemeris, ... }:
    ephemeris.lib.mkFlake { inherit inputs; } {
      imports = [
        ephemeris.presets.inomotech
        ./ephemeris.nix
        ./nix/cornucopia.nix
        ./nix/rust-packages.nix
      ];

      granary.ephemeris.rust.packages = {
        calf-granary-client = { };
        calf-granary-server = { };
      };
    };
}
