{ lib, ... }:
{
  perSystem =
    { granaryCrane, pkgs, ... }:
    let
      services = import ./services.nix { inherit lib pkgs; };
      inherit (granaryCrane) craneLib common cargoArtifacts;
    in
    {
      checks.integration = craneLib.cargoNextest (
        common
        // {
          inherit cargoArtifacts;
          pname = "calf-granary-integration";
          cargoNextestExtraArgs = "--locked --workspace --run-ignored only";
          nativeBuildInputs = common.nativeBuildInputs ++ [
            pkgs.curl
            services.postgresql
          ];
          preCheck = ''
            services=$(mktemp -d)
            initdb --pgdata "$services/postgres" --username postgres --auth trust --no-sync >/dev/null
            pg_ctl --pgdata "$services/postgres" --options "-k $services -c listen_addresses=" \
              --log "$services/postgres.log" --wait start
            s3_port=$((20000 + RANDOM % 20000))
            mkdir "$services/seaweedfs"
            (cd "$services/seaweedfs" && exec ${services.seaweedfs { port = "$s3_port"; }}) \
              >"$services/seaweedfs.log" 2>&1 &
            seaweedfs=$!

            export GRANARY_DATABASE_URL="host=$services user=postgres dbname=postgres"
            export AWS_ENDPOINT="http://127.0.0.1:$s3_port"
            ${services.s3Exports}
            ready=
            for _ in $(seq 120); do
              if ${services.createBuckets [ "granary-test" ]} 2>/dev/null; then
                ready=1
                break
              fi
              sleep 1
            done
            if [ -z "$ready" ]; then
              cat "$services/seaweedfs.log"
              exit 1
            fi
          '';
          postCheck = ''
            kill "$seaweedfs"
            pg_ctl --pgdata "$services/postgres" --mode fast stop
          '';
        }
      );
    };
}
