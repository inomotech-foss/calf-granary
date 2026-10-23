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
      signing_key = "@state@/nix-main.key";
    };
    nix-pr = {
      store = "short";
      retention = "7d";
      protocol = "nix";
      signing_key = "@state@/nix-pr.key";
      fallback = "nix-main";
    };
  };
in
{
  ephemeris.services = {
    garage = {
      enable = true;
      buckets = lib.mapAttrsToList (_: store: store.bucket) stores ++ [ "granary-test" ];
    };
    postgres = {
      enable = true;
      port = 15432;
      tls = true;
    };
  };

  perSystem =
    { config, pkgs, ... }:
    let
      server = lib.getExe' config.packages.calf-granary-server "calf-granary-server";
      services = import ./services.nix { inherit lib pkgs; };
      granaryConfig = (pkgs.formats.toml { }).generate "granary-dev.toml" { inherit stores caches; };
      env = lib.concatMapStrings (process: ". .ephemeris/services/${process}/exports.env && ") [
        "granary"
        "garage-setup"
      ];

      setup = pkgs.writeShellApplication {
        name = "granary-dev-setup";
        runtimeInputs = [
          pkgs.coreutils
          pkgs.gnused
          pkgs.nix
        ];
        text = ''
          state=$(realpath "$EPHEMERIS_SERVICE_STATE")
          umask 077
          secret="$state/token-secret"
          if [ ! -f "$secret" ]; then
            od -An -tx1 -N32 /dev/urandom | tr -d ' \n' >"$secret"
          fi

          for cache in nix-main nix-pr; do
            key="$state/$cache.key"
            if [ ! -f "$key" ]; then
              nix --extra-experimental-features nix-command key generate-secret \
                --key-name "granary-dev-$cache-1" >"$key"
            fi
          done
          sed "s|@state@|$state|g" ${granaryConfig} >"$state/config.toml"

          cat >"$state/exports.env" <<EOF
          GRANARY_DATABASE_CA=$(realpath "$PGSSLROOTCERT")
          GRANARY_TOKEN_SECRET=$(cat "$secret")
          EOF
        '';
      };
    in
    {
      ephemeris.processes = {
        granary = {
          command = lib.getExe setup;
          oneShot = true;
          after = [ "postgres" ];
          exports = services.clientEnvironment // {
            GRANARY_CONFIG = "{state}/config.toml";
            GRANARY_DATABASE_URL = "host=127.0.0.1 port=15432 user=postgres dbname=postgres sslmode=require";
            GRANARY_LISTEN = "127.0.0.1:18080";
            AWS_ALLOW_HTTP = "true";
          };
        };
        migrate = {
          command = "set -a && ${env}exec ${server} migrate";
          oneShot = true;
          after = [
            "granary"
            "postgres"
          ];
        };
        serve = {
          command = "set -a && ${env}exec ${server} serve";
          after = [
            "garage"
            "garage-setup"
            "granary"
            "migrate"
          ];
          ports.http = 18080;
          readiness.http = {
            port = "http";
            path = "/readyz";
          };
        };
        worker = {
          command = "set -a && ${env}exec ${server} worker";
          after = [
            "garage"
            "garage-setup"
            "granary"
            "migrate"
          ];
        };
      };
    };
}
