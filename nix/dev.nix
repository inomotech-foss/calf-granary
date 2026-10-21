{ lib, ... }:
let
  stores = {
    long.bucket = "granary-long";
    short.bucket = "granary-short";
  };
  caches = {
    main = {
      store = "long";
      retention = "30d";
      role = "sstate";
    };
    pr = {
      store = "short";
      retention = "7d";
      role = "sstate";
      fallback = "main";
    };
    downloads = {
      store = "long";
      retention = "365d";
      role = "downloads";
    };
    hash-main = {
      protocol = "hashserv";
      retention = "30d";
    };
    hash-pr = {
      protocol = "hashserv";
      retention = "7d";
      fallback = "hash-main";
    };
    nix-main = {
      store = "long";
      retention = "30d";
      protocol = "nix";
      signing_key = "@dev@/nix-main.key";
    };
    nix-pr = {
      store = "short";
      retention = "7d";
      protocol = "nix";
      signing_key = "@dev@/nix-pr.key";
      fallback = "nix-main";
    };
  };
  buckets = lib.mapAttrsToList (_: store: store.bucket) stores ++ [ "granary-test" ];
in
{
  ephemeris.gitignore.entries = [ "/.dev" ];

  perSystem =
    { config, pkgs, ... }:
    let
      server = lib.getExe' config.packages.calf-granary-server "calf-granary-server";
      services = import ./services.nix { inherit lib pkgs; };
      inherit (services) curl postgresql;
      granaryConfig = (pkgs.formats.toml { }).generate "granary-dev.toml" { inherit stores caches; };

      postgres = pkgs.writeShellScript "granary-dev-postgres" ''
        data="$GRANARY_DEV_DIR/postgres"
        if [ ! -f "$data/PG_VERSION" ]; then
          ${postgresql}/bin/initdb --pgdata "$data" --username postgres --auth trust --no-sync
        fi
        ${services.postgresTls} "$GRANARY_DEV_DIR/tls"
        exec ${postgresql}/bin/postgres -D "$data" -k "$GRANARY_DEV_DIR" \
          -c listen_addresses=127.0.0.1 -c port=15432 \
          ${services.postgresTlsOptions "$GRANARY_DEV_DIR/tls"}
      '';

      seaweedfs = pkgs.writeShellScript "granary-dev-seaweedfs" ''
        mkdir -p "$GRANARY_DEV_DIR/seaweedfs"
        cd "$GRANARY_DEV_DIR/seaweedfs"
        exec ${services.seaweedfs { port = "18333"; }}
      '';

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
            command = services.createBuckets buckets;
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
          pkgs.gnused
          pkgs.nix
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

          for cache in nix-main nix-pr; do
            key="$GRANARY_DEV_DIR/$cache.key"
            if [ ! -f "$key" ]; then
              nix --extra-experimental-features nix-command key generate-secret \
                --key-name "granary-dev-$cache-1" >"$key"
            fi
          done
          sed "s|@dev@|$GRANARY_DEV_DIR|g" ${granaryConfig} >"$GRANARY_DEV_DIR/config.toml"

          cat >"$GRANARY_DEV_DIR/env" <<EOF
          export GRANARY_CONFIG=$GRANARY_DEV_DIR/config.toml
          export GRANARY_DATABASE_URL="host=127.0.0.1 port=15432 user=postgres dbname=postgres sslmode=require"
          export GRANARY_DATABASE_CA=$GRANARY_DEV_DIR/tls/ca.crt
          export GRANARY_LISTEN=127.0.0.1:18080
          export GRANARY_TOKEN_SECRET=$(cat "$secret")
          export AWS_ENDPOINT=http://127.0.0.1:18333
          ${services.s3Exports}
          ${services.bitbakeExports}
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
