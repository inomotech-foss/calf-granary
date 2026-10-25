{ lib, ... }:
{
  ephemeris.checks.integration = {
    command = lib.concatStringsSep " && " [
      "export GRANARY_DATABASE_URL=\"host=127.0.0.1 port=$PGPORT user=$PGUSER dbname=$PGDATABASE sslmode=require\""
      "export GRANARY_DATABASE_CA=$PGSSLROOTCERT"
      "cargo nextest run --archive-file \"$EPHEMERIS_RUST_TESTS\" --workspace-remap . --extract-to . --tool-config-file \"ephemeris:$EPHEMERIS_NEXTEST_CONFIG\" --run-ignored only"
    ];
    services = [
      "postgres"
      "garage-setup"
    ];
    report = {
      format = "junit";
      source.file = "target/nextest/default/junit.xml";
    };
  };

  perSystem =
    { config, pkgs, ... }:
    {
      ephemeris.checks.integration = {
        tools = config.ephemeris.rust.tools ++ [ pkgs.nix ];
        env =
          config.ephemeris.rust.env
          // import ./clients.nix { inherit lib pkgs; }
          // {
            AWS_ALLOW_HTTP = "true";
            GRANARY_CLIENT = lib.getExe' config.packages.calf-granary-client "calf-granary-client";
          };
      };
    };
}
