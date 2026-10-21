{ lib, ... }:
let
  caches = {
    main = {
      bucket = "granary-main";
      retention = "30d";
    };
    pr = {
      bucket = "granary-pr";
      retention = "7d";
      fallback = "main";
    };
  };
  buckets = lib.unique (lib.mapAttrsToList (_: cache: cache.bucket) caches);
in
{
  ephemeris.gitignore.entries = [ "/.dev" ];

  perSystem =
    { config, pkgs, ... }:
    let
      server = lib.getExe' config.packages.calf-granary-server "calf-granary-server";
      postgresql = pkgs.postgresql_18;
      granaryConfig = (pkgs.formats.toml { }).generate "granary-dev.toml" { inherit caches; };

      postgres = pkgs.writeShellScript "granary-dev-postgres" ''
        data="$GRANARY_DEV_DIR/postgres"
        if [ ! -f "$data/PG_VERSION" ]; then
          ${postgresql}/bin/initdb --pgdata "$data" --username postgres --auth trust --no-sync
        fi
        exec ${postgresql}/bin/postgres -D "$data" -k "$GRANARY_DEV_DIR" \
          -c listen_addresses=127.0.0.1 -c port=15432
      '';

      s3Config = (pkgs.formats.json { }).generate "granary-dev-s3.json" {
        identities = [
          {
            name = "dev";
            credentials = [
              {
                accessKey = "dev";
                secretKey = "dev";
              }
            ];
            actions = [ "Admin" ];
          }
        ];
      };

      seaweedfs = pkgs.writeShellScript "granary-dev-seaweedfs" ''
        mkdir -p "$GRANARY_DEV_DIR/seaweedfs"
        cd "$GRANARY_DEV_DIR/seaweedfs"
        exec ${lib.getExe' pkgs.seaweedfs "weed"} server -dir=. -ip=127.0.0.1 -ip.bind=127.0.0.1 \
          -master.port=19333 -volume.port=19080 -volume.max=100 \
          -filer -filer.port=19888 -s3 -s3.port=18333 -s3.config=${s3Config}
      '';

      createBuckets = pkgs.writeShellScript "granary-dev-buckets" ''
        s3() {
          ${curl} --aws-sigv4 "aws:amz:$AWS_REGION:s3" \
            --user "$AWS_ACCESS_KEY_ID:$AWS_SECRET_ACCESS_KEY" "$@"
        }
        for bucket in ${lib.escapeShellArgs buckets}; do
          s3 --head "$AWS_ENDPOINT/$bucket" >/dev/null 2>&1 \
            || s3 --request PUT "$AWS_ENDPOINT/$bucket"
        done
      '';

      curl = "${lib.getExe pkgs.curl} --fail --silent --show-error";

      readiness = probe: {
        readiness_probe = probe // {
          period_seconds = 1;
          failure_threshold = 60;
        };
      };

      after = conditions: {
        depends_on = lib.mapAttrs (_: condition: { inherit condition; }) conditions;
      };

      afterSetup = after {
        buckets = "process_completed_successfully";
        migrate = "process_completed_successfully";
        postgres = "process_healthy";
        seaweedfs = "process_healthy";
      };

      processCompose = (pkgs.formats.yaml { }).generate "granary-dev.yaml" {
        version = "0.5";
        processes = {
          postgres = {
            command = postgres;
            shutdown.signal = 2;
          }
          // readiness {
            exec.command = "${postgresql}/bin/pg_isready --host 127.0.0.1 --port 15432 --username postgres --dbname postgres";
          };
          seaweedfs = {
            command = seaweedfs;
          }
          // readiness { exec.command = "${curl} http://127.0.0.1:18333/healthz"; };
          buckets = {
            command = createBuckets;
          }
          // after { seaweedfs = "process_healthy"; };
          migrate = {
            command = "${server} migrate";
          }
          // after { postgres = "process_healthy"; };
          serve = {
            command = "${server} serve";
          }
          // afterSetup
          // readiness { exec.command = "${curl} http://127.0.0.1:18080/readyz"; };
          worker = {
            command = "${server} worker";
          }
          // afterSetup;
        };
      };

      dev = pkgs.writeShellApplication {
        name = "granary-dev";
        runtimeInputs = [
          pkgs.coreutils
          pkgs.git
          pkgs.process-compose
        ];
        text = ''
          root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
          export GRANARY_DEV_DIR="''${GRANARY_DEV_DIR:-$root/.dev}"
          umask 077
          mkdir -p "$GRANARY_DEV_DIR"
          secret="$GRANARY_DEV_DIR/token-secret"
          if [ ! -f "$secret" ]; then
            od -An -tx1 -N32 /dev/urandom | tr -d ' \n' >"$secret"
          fi

          cat >"$GRANARY_DEV_DIR/env" <<EOF
          export GRANARY_CONFIG=${granaryConfig}
          export GRANARY_DATABASE_URL="host=$GRANARY_DEV_DIR port=15432 user=postgres dbname=postgres"
          export GRANARY_LISTEN=127.0.0.1:18080
          export GRANARY_TOKEN_SECRET=$(cat "$secret")
          export AWS_ENDPOINT=http://127.0.0.1:18333
          export AWS_ALLOW_HTTP=true
          export AWS_REGION=us-east-1
          export AWS_ACCESS_KEY_ID=dev
          export AWS_SECRET_ACCESS_KEY=dev
          EOF
          # shellcheck source=/dev/null
          source "$GRANARY_DEV_DIR/env"

          exec process-compose up --config ${processCompose} --disable-dotenv \
            --use-uds --unix-socket "$GRANARY_DEV_DIR/process-compose.sock" \
            --ordered-shutdown "$@"
        '';
      };
    in
    {
      apps.dev = {
        type = "app";
        program = lib.getExe dev;
        meta.description = "Run granary with Postgres and seaweedfs on this machine";
      };
    };
}
