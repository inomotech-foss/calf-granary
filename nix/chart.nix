{ lib, ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      treefmt.settings.global.excludes = [ "chart/*/templates/*" ];

      checks.chart =
        pkgs.runCommand "calf-granary-chart-check"
          {
            nativeBuildInputs = [ pkgs.kubernetes-helm ];
            chart = lib.fileset.toSource {
              root = ../chart;
              fileset = ../chart/calf-granary;
            };
          }
          ''
            export HOME=$TMPDIR HELM_DATA_HOME=$TMPDIR/helm HELM_CONFIG_HOME=$TMPDIR/helm
            values=$chart/calf-granary/ci-values.yaml
            helm lint --strict $chart/calf-granary --values $values
            helm template granary $chart/calf-granary --values $values >/dev/null
            touch $out
          '';
    };
}
