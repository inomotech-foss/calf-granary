{ lib, ... }:
{
  perSystem =
    { config, pkgs, ... }:
    let
      server = config.packages.calf-granary-server-x86_64-unknown-linux-musl;
    in
    {
      packages = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
        calf-granary-server-x86_64-linux-image = pkgs.dockerTools.streamLayeredImage {
          name = "calf-granary-server";
          tag = "${server.version}-x86_64-linux";
          architecture = "amd64";
          contents = [ pkgs.cacert ];
          config = {
            Entrypoint = [ "${server}/bin/calf-granary-server" ];
            User = "65534:65534";
            Env = [ "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt" ];
          };
        };
      };
    };
}
