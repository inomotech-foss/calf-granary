{ lib, ... }:
{
  ephemeris.checks.integration = {
    command = lib.concatStringsSep " && " [
      "export GRANARY_DATABASE_URL=\"host=127.0.0.1 port=$PGPORT user=$PGUSER dbname=$PGDATABASE sslmode=require\""
      "export GRANARY_DATABASE_CA=$PGSSLROOTCERT"
      "cargo --quiet --locked build --package calf-granary-client"
      "cargo --locked nextest run --workspace --run-ignored only"
    ];
    services = [
      "postgres"
      "garage-setup"
    ];
  };

  perSystem =
    {
      config,
      pkgs,
      rustCrane,
      ...
    }:
    let
      clients = import ./clients.nix { inherit lib pkgs; };
      inherit (rustCrane.stable) craneLib shared cargoArtifacts;
    in
    {
      ephemeris.checks.integration = {
        tools = config.ephemeris.rust.nativeInputs.nativeBuildInputs ++ [
          config.ephemeris.rust.toolchain
          pkgs.cargo-nextest
          pkgs.nix
        ];
        env = clients // {
          AWS_ALLOW_HTTP = "true";
        };
        builder =
          {
            name,
            src,
            nativeBuildInputs,
            runCheck,
            env,
            ...
          }:
          craneLib.mkCargoDerivation (
            {
              inherit src cargoArtifacts nativeBuildInputs;
              pname = "ephemeris-check-${name}";
              version = "0.0.0";
              buildPhaseCargoCommand = runCheck;
              installPhaseCommand = "touch $out";
              doCheck = false;
              doInstallCargoArtifacts = false;
            }
            // shared
            // env
          );
      };
    };
}
