{ lib, ... }:

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
      workspace.members = lib.genAttrs [ "calf-granary-client" "calf-granary-server" ] (_: {
        targets = [ "x86_64-unknown-linux-musl" ];
      });
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
      units.calf-granary-chart = {
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
  };

  perSystem =
    { pkgs, ... }:
    {
      ephemeris.images.calf-granary-server.contents = [ pkgs.cacert ];
    };
}
