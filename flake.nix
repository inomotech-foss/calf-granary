{
  description = "Build caches for CI runners";

  inputs = {
    ephemeris.url = "git+https://codeberg.org/inomotech/ephemeris";
  };

  outputs =
    inputs@{ ephemeris, ... }:
    ephemeris.lib.mkFlake { inherit inputs; } {
      imports = [
        ephemeris.presets.inomotech
        ./ephemeris.nix
        ./nix/rust-packages.nix
      ];

      granary.ephemeris.rust.packages = {
        calf-granary-client = { };
        calf-granary-server = { };
      };
    };
}
