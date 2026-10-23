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

  s3Exports = lib.concatStringsSep "\n" [
    "export AWS_ACCESS_KEY_ID=GK000000000000000000000000"
    "export AWS_SECRET_ACCESS_KEY=${lib.strings.replicate 64 "0"}"
    "export AWS_REGION=garage"
    "export AWS_ALLOW_HTTP=true"
  ];

  # Starts Garage in the background with one bucket and the s3Exports key.
  garage = pkgs.writeShellScript "granary-garage" ''
    set -eu
    dir=$1
    port=$2
    bucket=$3
    mkdir -p "$dir"
    cat >"$dir/garage.toml" <<EOF
    metadata_dir = "$dir/meta"
    data_dir = "$dir/data"
    db_engine = "sqlite"
    replication_factor = 1
    rpc_bind_addr = "127.0.0.1:$((port + 1))"
    rpc_public_addr = "127.0.0.1:$((port + 1))"
    rpc_secret = "$(${lib.getExe pkgs.openssl} rand -hex 32)"

    [s3_api]
    s3_region = "garage"
    api_bind_addr = "127.0.0.1:$port"
    EOF
    g() { ${lib.getExe pkgs.garage} -c "$dir/garage.toml" "$@"; }
    g server >"$dir/garage.log" 2>&1 &
    echo $! >"$dir/pid"
    for _ in $(seq 120); do
      g status >/dev/null 2>&1 && break
      sleep 1
    done
    ${s3Exports}
    {
      g layout assign -z test -c 1G "$(g node id -q | cut -d@ -f1)"
      g layout apply --version 1
      g bucket create "$bucket"
      g key import --yes "$AWS_ACCESS_KEY_ID" "$AWS_SECRET_ACCESS_KEY"
      g bucket allow --read --write --owner "$bucket" --key "$AWS_ACCESS_KEY_ID"
    } >>"$dir/setup.log" 2>&1 || {
      cat "$dir/garage.log" "$dir/setup.log"
      exit 1
    }
  '';
}
