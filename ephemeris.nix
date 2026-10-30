{ lib, ... }:

let
  clientTargets = [
    "x86_64-unknown-linux-musl"
    "aarch64-unknown-linux-musl"
  ];
  clientAsset = target: "calf-granary-client-${target}";
in
{
  ephemeris = {
    project = {
      name = "calf-granary";
      description = "Build caches for CI runners";
      license = lib.licenses.agpl3Plus;
    };
    repository = {
      forge = "forgejo";
      forgejo.branchRules.rules.main = {
        push = "whitelist";
        pushWhitelistUsernames = [ "scootermon" ];
      };
    };
    commitMessage.conventional.allowScopes = false;
    rust = {
      enable = true;
      workspace.members = {
        calf-granary-client.targets = clientTargets;
        calf-granary-server.targets = [ "x86_64-unknown-linux-musl" ];
      };
      cornucopia.calf-granary-server-db = {
        migrations = "packages/server/migrations";
        queries = "packages/server/queries";
        destination = "packages/server-db";
        description = "Generated Postgres bindings for the server queries";
        keywords = [
          "cache"
          "ci"
          "nix"
          "yocto"
        ];
      };
    };
    typescript.enable = true;
    direnv.watch = [
      "ephemeris.nix"
      "dev.nix"
    ];
    release = {
      enable = true;
      units.calf-granary-client.tagPrefix = "client-v";
      units.calf-granary-server.tagPrefix = "server-v";
      units.calf-granary-server-db.tagPrefix = "server-db-v";
      artifacts.calf-granary-client = lib.listToAttrs (
        lib.concatMap (target: [
          {
            name = target;
            value.asset = {
              package = "calf-granary-client-assets";
              output = clientAsset target;
              filename = clientAsset target;
              format = "binary";
              variant = target;
            };
          }
          {
            name = "${target}-sha256";
            value.asset = {
              package = "calf-granary-client-assets";
              output = "${clientAsset target}.sha256";
              filename = "${clientAsset target}.sha256";
              format = "sha256";
              variant = target;
            };
          }
        ]) clientTargets
      );
      units.calf-granary-action = {
        tagPrefix = "action-v";
        target.npm = {
          manifest = "package.json";
          publish = false;
        };
        source = [
          "action"
          "action.yaml"
        ];
        changelog = "action/CHANGELOG.md";
        dependencies = [ "calf-granary-client" ];
        pins.calf-granary-client = {
          path = "action/client.json";
          key = [ "version" ];
          digests = lib.genAttrs clientTargets (target: {
            path = "action/client.json";
            key = [
              "digests"
              target
            ];
            algorithm = "sha256";
          });
        };
      };
      units.calf-granary-chart = {
        tagPrefix = "chart-v";
        target.document.path = "charts/calf-granary/Chart.yaml";
        dependencies = [ "calf-granary-server" ];
        pins.calf-granary-server = {
          path = "charts/calf-granary/Chart.yaml";
          key = [ "appVersion" ];
        };
      };
    };
    releaseWorkflow.enable = true;
    images.calf-granary-server = {
      package = "calf-granary-server";
      platforms = [ "linux/amd64" ];
      user = {
        name = "nobody";
        uid = 65534;
        gid = 65534;
      };
      environment.SSL_CERT_FILE = "/etc/ssl/certs/ca-bundle.crt";
      release.unit = "calf-granary-server";
    };
    charts.calf-granary = {
      lint.values = [ "charts/calf-granary/ci-values.yaml" ];
      release.unit = "calf-granary-chart";
    };
    # PN is the recipe name in BitBake's hash equivalence protocol.
    typos.default.extendWords.pn = "pn";
    typos.default.extendWords.restat = "restat";
  };

  perSystem =
    { config, pkgs, ... }:
    {
      ephemeris.images.calf-granary-server.contents = [ pkgs.cacert ];
      packages = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
        calf-granary-client-assets = pkgs.runCommand "calf-granary-client-assets" { } (
          ''
            mkdir $out
            cd $out
          ''
          + lib.concatMapStrings (target: ''
            cp ${config.packages.${clientAsset target}}/bin/calf-granary-client ${clientAsset target}
            sha256sum ${clientAsset target} > ${clientAsset target}.sha256
          '') clientTargets
        );
      };
    };
}
