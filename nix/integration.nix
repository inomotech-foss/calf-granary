{ config, lib, ... }:
let
  source = config.ephemeris.internal.source;
in
{
  perSystem =
    {
      config,
      pkgs,
      rustCrane,
      ...
    }:
    let
      services = import ./services.nix { inherit lib pkgs; };
      inherit (rustCrane.stable) craneLib shared cargoArtifacts;
      inherit (config.ephemeris.rust.nativeInputs) nativeBuildInputs;
    in
    {
      checks.integration = craneLib.cargoNextest (
        shared
        // {
          inherit cargoArtifacts;
          src = source;
          version = "0.0.0";
          pname = "calf-granary-integration";
          cargoNextestExtraArgs = "--locked --workspace --run-ignored only";
          nativeBuildInputs = nativeBuildInputs ++ [
            pkgs.nix
            services.postgresql
          ];
          preCheck = ''
            cargo build --locked ''${CARGO_PROFILE:+--profile $CARGO_PROFILE} --package calf-granary-client
            services=$(mktemp -d)
            initdb --pgdata "$services/postgres" --username postgres --auth trust --no-sync >/dev/null
            s3_port=$((20000 + RANDOM % 20000))
            pg_port=$((s3_port + 5))
            ${services.postgresTls} "$services/tls"
            pg_ctl --pgdata "$services/postgres" --log "$services/postgres.log" --wait start \
              --options "-k $services -c listen_addresses=127.0.0.1 -c port=$pg_port ${services.postgresTlsOptions "$services/tls"}"
            ${services.garage} "$services/garage" "$s3_port" granary-test
            export GRANARY_DATABASE_URL="host=127.0.0.1 port=$pg_port user=postgres dbname=postgres sslmode=require"
            export GRANARY_DATABASE_CA="$services/tls/ca.crt"
            export AWS_ENDPOINT_URL="http://127.0.0.1:$s3_port"
            ${services.s3Exports}
            ${services.clientExports}
          '';
          postCheck = ''
            kill "$(cat "$services/garage/pid")"
            pg_ctl --pgdata "$services/postgres" --mode fast stop
          '';
        }
      );
    };
}
