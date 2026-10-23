{ lib, pkgs }:
let
  # The hash equivalence client that the tests speak to granary with.
  bitbake = pkgs.fetchFromGitHub {
    owner = "openembedded";
    repo = "bitbake";
    rev = "3ccdabf1ba2b65dde0025ad04f84b80c02a1fd6e";
    hash = "sha256-JXYnori7Ibd/2qjiLzbkpXwRMucq1Ly3VlI7pWOOhUE=";
  };
  python = pkgs.python3.withPackages (packages: [ packages.websockets ]);
in
# Clients that the end-to-end tests drive the server with.
{
  BITBAKE_ROOT = "${bitbake}";
  BITBAKE_PYTHON = lib.getExe python;
  GRANARY_NODE = lib.getExe pkgs.nodejs_24;
}
