{ lib, pkgs }:
rec {
  postgresql = pkgs.postgresql_18;

  # The hash equivalence client that the tests speak to granary with.
  bitbake = pkgs.fetchFromGitHub {
    owner = "openembedded";
    repo = "bitbake";
    rev = "3ccdabf1ba2b65dde0025ad04f84b80c02a1fd6e";
    hash = "sha256-JXYnori7Ibd/2qjiLzbkpXwRMucq1Ly3VlI7pWOOhUE=";
  };

  python = pkgs.python3.withPackages (packages: [ packages.websockets ]);

  # Clients that the end-to-end tests drive the server with.
  clientExports = lib.concatStringsSep "\n" [
    "export BITBAKE_ROOT=${bitbake}"
    "export BITBAKE_PYTHON=${lib.getExe python}"
    "export GRANARY_NODE=${lib.getExe pkgs.nodejs_24}"
  ];

  curl = "${lib.getExe pkgs.curl} --fail --silent --show-error";

  # A CA and a certificate for 127.0.0.1, created once per directory.
  postgresTls = pkgs.writeShellScript "granary-postgres-tls" ''
    set -eu
    dir=$1
    [ -f "$dir/server.crt" ] && exit 0
    mkdir -p "$dir"
    cd "$dir"
    openssl=${lib.getExe pkgs.openssl}
    $openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
      -days 3650 -subj /CN=granary-test-ca -keyout ca.key -out ca.crt \
      -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign
    $openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
      -subj /CN=127.0.0.1 -keyout server.key -out server.csr
    printf 'subjectAltName=IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' >server.ext
    $openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
      -days 3650 -extfile server.ext -out server.crt
    chmod 600 server.key
  '';

  postgresTlsOptions =
    dir: "-c ssl=on -c ssl_cert_file=${dir}/server.crt -c ssl_key_file=${dir}/server.key";

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
