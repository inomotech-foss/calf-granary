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
    charts.calf-granary.lint.values = [ "charts/calf-granary/ci-values.yaml" ];
    # PN is the recipe name in BitBake's hash equivalence protocol.
    typos.default.extendWords.pn = "pn";
  };
}
