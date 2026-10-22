{ lib, ... }:
{
  ephemeris.charts.calf-granary = { };

  perSystem =
    { pkgs, ... }:
    {
      checks.chart =
        pkgs.runCommand "calf-granary-chart-check"
          {
            nativeBuildInputs = [ pkgs.kubernetes-helm ];
            chart = lib.fileset.toSource {
              root = ../charts;
              fileset = ../charts/calf-granary;
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
