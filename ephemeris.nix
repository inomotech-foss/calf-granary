{ lib, withSystem, ... }:

{
  ephemeris = {
    project = {
      name = "calf-granary";
      description = "Build caches for CI runners";
      license = lib.licenses.agpl3Plus;
    };
    repository.forge = "forgejo";
    commitMessage.format = "conventional";
    rust = {
      enable = true;
      workspace.members = lib.genAttrs [ "calf-granary-client" "calf-granary-server" ] (_: {
        targets = [ "x86_64-unknown-linux-musl" ];
      });
    };
    typescript = {
      enable = true;
      pnpm.version = 11;
      dependencies.hash = "sha256-kb3JSodjfCXITftZb4/IDKUFFGRsdKeqAREE2NYAb2E=";
    };
    images.calf-granary-server = {
      package = "calf-granary-server";
      platforms = [ "linux/amd64" ];
      contents = [ (withSystem "x86_64-linux" ({ pkgs, ... }: pkgs.cacert)) ];
      user = {
        name = "nobody";
        uid = 65534;
        gid = 65534;
      };
      environment.SSL_CERT_FILE = "/etc/ssl/certs/ca-bundle.crt";
    };
    charts.calf-granary.lint.values = [ "charts/calf-granary/ci-values.yaml" ];
    # PN is the recipe name in BitBake's hash equivalence protocol.
    typos.default.extendWords.pn = "pn";
  };

  perSystem.treefmt.settings.formatter.yamlfmt.excludes = [ "pnpm-lock.yaml" ];
}
