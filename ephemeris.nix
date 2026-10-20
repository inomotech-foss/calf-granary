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
    rust.enable = true;
  };
}
