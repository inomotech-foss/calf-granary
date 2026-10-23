{ lib, ... }:

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
    typescript.enable = true;
    images.calf-granary-server = {
      package = "calf-granary-server";
      platforms = [ "linux/amd64" ];
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

  perSystem =
    { pkgs, ... }:
    {
      ephemeris.images.calf-granary-server.contents = [ pkgs.cacert ];
    };
}
