{ lib, ... }:
{
  ephemeris.gitignore.entries = [ "/node_modules" ];

  perSystem =
    { pkgs, ... }:
    let
      nodejs = pkgs.nodejs_24;
      nodeModules = pkgs.importNpmLock.buildNodeModules {
        inherit nodejs;
        npmRoot = lib.fileset.toSource {
          root = ../.;
          fileset = lib.fileset.unions [
            ../package.json
            ../package-lock.json
          ];
        };
      };
      source = lib.fileset.toSource {
        root = ../.;
        fileset = lib.fileset.unions [
          ../action
          ../package.json
          ../tsconfig.json
        ];
      };
    in
    {
      checks.action =
        pkgs.runCommand "calf-granary-action-check"
          {
            nativeBuildInputs = [ nodejs ];
          }
          ''
            cp -r ${source}/. .
            ln -s ${nodeModules}/node_modules node_modules
            node node_modules/typescript/bin/tsc --project tsconfig.json
            node --test "action/*.test.ts"
            touch $out
          '';
    };
}
