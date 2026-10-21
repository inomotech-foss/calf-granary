{ lib, pkgs }:
rec {
  postgresql = pkgs.postgresql_18;

  curl = "${lib.getExe pkgs.curl} --fail --silent --show-error";

  s3Config = (pkgs.formats.json { }).generate "granary-s3.json" {
    identities = [
      {
        name = "dev";
        credentials = [
          {
            accessKey = "dev";
            secretKey = "dev";
          }
        ];
        actions = [ "Admin" ];
      }
    ];
  };

  s3Exports = lib.concatStringsSep "\n" [
    "export AWS_ACCESS_KEY_ID=dev"
    "export AWS_SECRET_ACCESS_KEY=dev"
    "export AWS_REGION=us-east-1"
    "export AWS_ALLOW_HTTP=true"
  ];

  # weed server keeps gRPC sockets in /tmp, named after the port.
  seaweedfs =
    { port }:
    lib.concatStringsSep " " [
      "${lib.getExe' pkgs.seaweedfs "weed"} server -dir=. -ip=127.0.0.1 -ip.bind=127.0.0.1"
      "-master.port=$((${port} + 1)) -volume.port=$((${port} + 2)) -volume.max=100"
      "-filer -filer.port=$((${port} + 3)) -filer.localSocket=filer.sock"
      "-s3 -s3.port=${port} -s3.localSocket=s3.sock -s3.config=${s3Config}"
      "-s3.port.iceberg=0 -s3.port.lance=0 -master.telemetry=false"
    ];

  createBuckets =
    buckets:
    pkgs.writeShellScript "granary-buckets" ''
      s3() {
        ${curl} --aws-sigv4 "aws:amz:$AWS_REGION:s3" \
          --user "$AWS_ACCESS_KEY_ID:$AWS_SECRET_ACCESS_KEY" "$@"
      }
      for bucket in ${lib.escapeShellArgs buckets}; do
        s3 --head "$AWS_ENDPOINT/$bucket" >/dev/null 2>&1 \
          || s3 --request PUT "$AWS_ENDPOINT/$bucket"
      done
    '';
}
