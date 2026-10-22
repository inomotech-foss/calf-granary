{
  config,
  lib,
  ephemerisInputs,
  ...
}:
let
  toolchain = config.ephemeris.rust.toolchain;
  workspaceVersion = (lib.importTOML ../Cargo.toml).workspace.package.version;
  # Target system names and the nixpkgs cross package sets that build for them.
  targets = {
    x86_64-linux = "musl64";
  };
in
{
  ephemeris.rust.toolchain.targets = [ "x86_64-unknown-linux-musl" ];

  perSystem =
    { pkgs, system, ... }:
    let
      build =
        target: crossName:
        let
          cross = pkgs.pkgsCross.${crossName}.appendOverlays [
            ephemerisInputs.rust-overlay.overlays.default
          ];
          craneLib = (ephemerisInputs.crane.mkLib cross).overrideToolchain (
            toolchainPkgs:
            toolchainPkgs.rust-bin.fromRustupToolchain {
              inherit (toolchain) channel components targets;
            }
          );
          craneSource = (ephemerisInputs.crane.mkLib pkgs).filterCargoSources;
          common = {
            src = lib.cleanSourceWith {
              src = ../.;
              name = "source";
              filter =
                path: type:
                craneSource path type
                || lib.any (suffix: lib.hasSuffix suffix path) [
                  ".sql"
                  ".py"
                  ".ts"
                ];
            };
            strictDeps = true;
            version = workspaceVersion;
            doCheck = false;
          };
          cargoArtifacts = craneLib.buildDepsOnly (
            common
            // {
              pname = "calf-granary-static-dependencies";
              cargoExtraArgs = "--locked --workspace";
            }
          );
          binary =
            package:
            craneLib.buildPackage (
              common
              // {
                inherit cargoArtifacts;
                pname = package;
                cargoExtraArgs = "--locked --package ${package}";
              }
            );
          server = binary "calf-granary-server";
        in
        {
          "calf-granary-client-${target}-static" = binary "calf-granary-client";
          "calf-granary-server-${target}-static" = server;
          "calf-granary-server-${target}-image" = pkgs.dockerTools.streamLayeredImage {
            name = "calf-granary-server";
            tag = "${workspaceVersion}-${target}";
            architecture = cross.go.GOARCH;
            contents = [ pkgs.cacert ];
            config = {
              Entrypoint = [ "${server}/bin/calf-granary-server" ];
              User = "65534:65534";
              Env = [ "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt" ];
            };
          };
        };
    in
    lib.optionalAttrs (lib.hasSuffix "-linux" system) {
      packages = lib.concatMapAttrs build targets;
    };
}
