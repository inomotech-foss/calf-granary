{
  config,
  lib,
  self,
  ephemerisInputs,
  ...
}:
let
  cfg = config.granary.ephemeris.rust;
  projectName = config.ephemeris.project.name;
  workspaceVersion = (lib.importTOML (self + "/Cargo.toml")).workspace.package.version;
in
{
  options.granary.ephemeris.rust.packages = lib.mkOption {
    type = lib.types.attrsOf (
      lib.types.submodule (
        { name, ... }:
        {
          options.package = lib.mkOption {
            type = lib.types.str;
            default = name;
            description = "The Cargo package to build.";
          };
        }
      )
    );
    default = { };
    description = "Cargo packages exposed as flake packages under the same name.";
  };

  config.perSystem =
    { config, pkgs, ... }:
    let
      rust = config.ephemeris.rust;
      craneLib = (ephemerisInputs.crane.mkLib pkgs).overrideToolchain rust.toolchain;
      common = {
        src = craneLib.cleanCargoSource self;
        strictDeps = true;
        version = workspaceVersion;
        inherit (rust.nativeInputs) nativeBuildInputs buildInputs;
      }
      // rust.nativeInputs.env;
      cargoArtifacts = craneLib.buildDepsOnly (
        common
        // {
          pname = "${projectName}-dependencies";
          cargoExtraArgs = "--locked --workspace";
        }
      );
    in
    {
      packages = lib.mapAttrs (
        _: package:
        craneLib.buildPackage (
          common
          // {
            inherit cargoArtifacts;
            pname = package.package;
            cargoExtraArgs = "--locked --package ${package.package}";
            doCheck = false;
          }
        )
      ) cfg.packages;
    };
}
