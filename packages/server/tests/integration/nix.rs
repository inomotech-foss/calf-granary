//! Serving Nix binary caches.

use std::fs;
use std::path::Path;
use std::process::Stdio;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;
use tokio::process::Command;

use crate::harness::{Server, Stack, client_bin};

const CACHES: &str = r#"
[caches.nix-main]
store = "test"
protocol = "nix"
signing_key = "$dir/main.key"
public = true

[caches.nix-pr]
store = "test"
protocol = "nix"
signing_key = "$dir/pr.key"
public = true
priority = 30
fallback = "nix-main"
"#;

const MAIN_KEY: &str = "main-1:gDQElRwG6hWQbzvWiPbOwXBTfsbJksJCSpVX/RYucmv/Qjzzg/\
                        1fQ4UimSnxIDFlz+sk9nb4Z1Ws41Rc6LhzIg==";

const PR_KEY: &str =
    "pr-1:gDQElRwG6hWQbzvWiPbOwXBTfsbJksJCSpVX/RYucmv/Qjzzg/1fQ4UimSnxIDFlz+sk9nb4Z1Ws41Rc6LhzIg==";

const HASH: &str = "0mdqa9w1p6cmli6976v4wi0sw9r4p5pr";

const OTHER: &str = "1mdqa9w1p6cmli6976v4wi0sw9r4p5pr";

fn nar() -> Vec<u8> {
    (0..1_000_000u32)
        .map(|n| u8::try_from(n.wrapping_mul(7919) % 251).unwrap())
        .collect()
}

fn info(nar: &[u8], nar_hash: &str) -> Value {
    json!({
        "store_path": format!("/nix/store/{HASH}-hello-2.12"),
        "references": [
            format!("/nix/store/{HASH}-hello-2.12"),
            format!("/nix/store/{OTHER}-glibc-2.40"),
        ],
        "deriver": format!("/nix/store/{OTHER}-hello-2.12.drv"),
        "system": "x86_64-linux",
        "ca": null,
        "nar_hash": nar_hash,
        "nar_size": nar.len(),
    })
}

fn sri(nar: &[u8]) -> String {
    format!("sha256-{}", STANDARD.encode(digest(&SHA256, nar)))
}

async fn upload(server: &Server, token: Option<&str>, info: &Value, nar: Vec<u8>) -> StatusCode {
    let mut request = server
        .http()
        .put(server.url("_api/nix/nix-main/upload"))
        .body([format!("{info}\n").into_bytes(), nar].concat());
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    request.send().await.unwrap().status()
}

async fn text(server: &Server, path: &str) -> (StatusCode, String) {
    let response = server.http().get(server.url(path)).send().await.unwrap();
    (response.status(), response.text().await.unwrap())
}

async fn missing(server: &Server, cache: &str, hashes: &[&str]) -> Value {
    server
        .http()
        .post(server.url(&format!("_api/nix/{cache}/missing")))
        .json(&json!({ "hashes": hashes }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn field<'a>(narinfo: &'a str, name: &str) -> &'a str {
    narinfo
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .unwrap_or_else(|| panic!("{name} is missing from\n{narinfo}"))
}

async fn verify(server: &Server, cache: &str, narinfo: &str) {
    let (_, public) = text(server, &format!("_api/nix/{cache}/public-key")).await;
    let (key_name, public) = public.split_once(':').unwrap();
    let references: Vec<String> = field(narinfo, "References")
        .split(' ')
        .map(|reference| format!("/nix/store/{reference}"))
        .collect();
    let fingerprint = format!(
        "1;{};{};{};{}",
        field(narinfo, "StorePath"),
        field(narinfo, "NarHash"),
        field(narinfo, "NarSize"),
        references.join(",")
    );
    let (sig_name, signature) = field(narinfo, "Sig").split_once(':').unwrap();
    assert_eq!(sig_name, key_name, "the cache's key signs");
    UnparsedPublicKey::new(&ED25519, STANDARD.decode(public).unwrap())
        .verify(fingerprint.as_bytes(), &STANDARD.decode(signature).unwrap())
        .unwrap();
}

async fn check_store_path(server: &Server, cache: &str, nar: &[u8]) {
    let (status, narinfo) = text(server, &format!("{cache}/{HASH}.narinfo")).await;
    assert_eq!(status, StatusCode::OK, "{cache} serves the narinfo");
    assert_eq!(
        field(&narinfo, "StorePath"),
        format!("/nix/store/{HASH}-hello-2.12"),
        "the store path"
    );
    assert_eq!(field(&narinfo, "Compression"), "zstd", "NARs are zstd");
    assert_eq!(
        field(&narinfo, "NarSize"),
        nar.len().to_string(),
        "the NAR size"
    );
    assert_eq!(
        field(&narinfo, "Deriver"),
        format!("{OTHER}-hello-2.12.drv"),
        "the deriver is a base name"
    );
    verify(server, cache, &narinfo).await;

    let url = field(&narinfo, "URL");
    let head = server
        .http()
        .head(server.url(&format!("{cache}/{url}")))
        .send()
        .await
        .unwrap();
    assert_eq!(
        head.headers()[CONTENT_LENGTH],
        field(&narinfo, "FileSize"),
        "HEAD reports the file size"
    );
    let response = server
        .http()
        .get(server.url(&format!("{cache}/{url}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{cache} serves the NAR");
    let file = response.bytes().await.unwrap();
    assert_eq!(
        field(&narinfo, "FileSize"),
        file.len().to_string(),
        "the file size matches"
    );
    assert_eq!(
        zstd::decode_all(file.as_ref()).unwrap(),
        nar,
        "the NAR decompresses to the upload"
    );
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn serves_uploaded_store_paths() {
    let stack = Stack::new(CACHES).await;
    stack.write("main.key", MAIN_KEY);
    stack.write("pr.key", PR_KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["nix-main"]).await;
    let nar = nar();
    let info = info(&nar, &sri(&nar));

    let wrong = self::info(&nar, &sri(b"other"));
    let cases = [
        (
            None,
            &info,
            Vec::new(),
            StatusCode::UNAUTHORIZED,
            "uploads need a token",
        ),
        (
            Some(token.as_str()),
            &wrong,
            nar.clone(),
            StatusCode::BAD_REQUEST,
            "the hash must match",
        ),
        (
            Some(token.as_str()),
            &info,
            nar.clone(),
            StatusCode::CREATED,
            "a new path is stored",
        ),
        (
            Some(token.as_str()),
            &info,
            Vec::new(),
            StatusCode::NO_CONTENT,
            "an existing path is kept",
        ),
    ];
    for (bearer, metadata, body, expected, reason) in cases {
        assert_eq!(
            upload(&server, bearer, metadata, body).await,
            expected,
            "{reason}"
        );
    }

    assert_eq!(
        missing(&server, "nix-main", &[HASH, OTHER]).await,
        json!({ "missing": [OTHER] }),
        "only the unknown path is missing"
    );
    assert_eq!(
        missing(&server, "nix-pr", &[HASH, OTHER]).await,
        json!({ "missing": [OTHER] }),
        "the fallback counts"
    );

    let (info_status, cache_info) = text(&server, "nix-pr/nix-cache-info").await;
    assert_eq!(info_status, StatusCode::OK, "the cache info exists");
    assert_eq!(
        cache_info, "StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 30\n",
        "the cache info"
    );

    for cache in ["nix-main", "nix-pr"] {
        check_store_path(&server, cache, &nar).await;
    }
    assert_eq!(
        text(&server, &format!("nix-main/{OTHER}.narinfo")).await.0,
        StatusCode::NOT_FOUND,
        "unknown paths are missing"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn serves_nars_that_a_files_cache_stored_whole() {
    let stack = Stack::new(&format!("{CACHES}\n[caches.files]\nstore = \"test\"\n")).await;
    stack.write("main.key", MAIN_KEY);
    stack.write("pr.key", PR_KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["files", "nix-main"]).await;
    let nar: Vec<u8> = nar().repeat(6);
    assert_eq!(
        put_basic(&server, &token, "files/nar", nar.clone()).await,
        StatusCode::CREATED,
        "the files cache stores the content whole"
    );
    assert_eq!(
        upload(&server, Some(&token), &info(&nar, &sri(&nar)), nar.clone()).await,
        StatusCode::CREATED,
        "the store path reuses the blob"
    );
    let (_, narinfo) = text(&server, &format!("nix-main/{HASH}.narinfo")).await;
    let file = get_bytes(&server, &format!("nix-main/{}", field(&narinfo, "URL"))).await;
    assert_eq!(
        zstd::decode_all(file.as_slice()).unwrap(),
        nar,
        "the whole content is compressed on the fly"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}

async fn head_status(server: &Server, path: &str) -> StatusCode {
    server
        .http()
        .head(server.url(path))
        .send()
        .await
        .unwrap()
        .status()
}

async fn get_bytes(server: &Server, path: &str) -> Vec<u8> {
    let response = server.http().get(server.url(path)).send().await.unwrap();
    response.bytes().await.unwrap().to_vec()
}

async fn put_basic(server: &Server, token: &str, path: &str, body: Vec<u8>) -> StatusCode {
    server
        .http()
        .put(server.url(path))
        .basic_auth("granary", Some(token))
        .body(body)
        .send()
        .await
        .unwrap()
        .status()
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn accepts_nix_copy_uploads() {
    let stack = Stack::new(CACHES).await;
    stack.write("main.key", MAIN_KEY);
    stack.write("pr.key", PR_KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["nix-main"]).await;
    let nar = nar();
    let file = zstd::encode_all(nar.as_slice(), 3).unwrap();
    let file_name = format!("nar/{}.nar.zst", "1".repeat(52));
    let narinfo = |nar_hash: &str| {
        format!(
            "StorePath: /nix/store/{HASH}-hello-2.12\nURL: {file_name}\nCompression: \
             zstd\nFileHash: sha256-{}\nFileSize: {}\nNarHash: {nar_hash}\nNarSize: \
             {}\nReferences: {OTHER}-glibc-2.40\nDeriver: unknown-deriver\n",
            STANDARD.encode(digest(&SHA256, &file)),
            file.len(),
            nar.len(),
        )
    };

    assert_eq!(
        put_basic(
            &server,
            &token,
            &format!("nix-main/{HASH}"),
            b"squat".to_vec()
        )
        .await,
        StatusCode::BAD_REQUEST,
        "files can't take the name of a store path"
    );
    let early = narinfo(&sri(&nar)).into_bytes();
    assert_eq!(
        put_basic(&server, &token, &format!("nix-main/{HASH}.narinfo"), early).await,
        StatusCode::BAD_REQUEST,
        "a narinfo needs its NAR file first"
    );
    assert_eq!(
        head_status(&server, &format!("nix-main/{file_name}")).await,
        StatusCode::NOT_FOUND,
        "nix copy sees that the NAR file is missing"
    );
    assert_eq!(
        put_basic(
            &server,
            &token,
            &format!("nix-main/{file_name}"),
            file.clone()
        )
        .await,
        StatusCode::CREATED,
        "the NAR file is stored"
    );
    assert_eq!(
        head_status(&server, &format!("nix-main/{file_name}")).await,
        StatusCode::OK,
        "nix copy sees the NAR file"
    );
    let wrong = narinfo(&sri(b"other")).into_bytes();
    assert_eq!(
        put_basic(&server, &token, &format!("nix-main/{HASH}.narinfo"), wrong).await,
        StatusCode::BAD_REQUEST,
        "the narinfo must match the NAR"
    );
    let right = narinfo(&sri(&nar)).into_bytes();
    assert_eq!(
        put_basic(&server, &token, &format!("nix-main/{HASH}.narinfo"), right).await,
        StatusCode::CREATED,
        "the store path is recorded"
    );

    let (status, recorded) = text(&server, &format!("nix-main/{HASH}.narinfo")).await;
    assert_eq!(status, StatusCode::OK, "the narinfo is served");
    assert_eq!(
        field(&recorded, "References"),
        format!("{OTHER}-glibc-2.40"),
        "the references"
    );
    verify(&server, "nix-main", &recorded).await;
    let response = server
        .http()
        .get(server.url(&format!("nix-main/{}", field(&recorded, "URL"))))
        .send()
        .await
        .unwrap();
    assert_eq!(
        zstd::decode_all(response.bytes().await.unwrap().as_ref()).unwrap(),
        nar,
        "the NAR is served"
    );
    let uploaded = get_bytes(&server, &format!("nix-main/{file_name}")).await;
    assert_eq!(
        zstd::decode_all(uploaded.as_slice()).unwrap(),
        nar,
        "the uploaded file's URL still serves zstd, as nix copy remembers it"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}

/// Runs Nix with its state below `dir` and the given store.
async fn run_nix(dir: &Path, store: &str, args: &[&str]) -> String {
    let output = Command::new(args.first().copied().unwrap_or("nix"))
        .args(args.get(1..).unwrap_or_default())
        .env("NIX_REMOTE", store)
        .env(
            "NIX_CONFIG",
            "experimental-features = nix-command\nnarinfo-cache-negative-ttl = 0\n",
        )
        .env("HOME", dir)
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Registers an output of a derivation in the store at `dir/source` without
/// building it, since chroot stores can't build on every platform.
async fn register_built(dir: &Path, store: &str) -> String {
    let expression = r#"
        let built = derivation { name = "built"; system = "riscv64-linux"; builder = "/bin/sh"; };
        in "${built.drvPath} ${built.outPath}"
    "#;
    let paths = run_nix(dir, store, &["nix", "eval", "--raw", "--expr", expression]).await;
    let (deriver, output) = paths.split_once(' ').unwrap();
    fs::write(
        dir.join("source").join(output.trim_start_matches('/')),
        "built",
    )
    .unwrap();
    let mut register = Command::new("nix-store")
        .arg("--register-validity")
        .env("NIX_REMOTE", store)
        .env("HOME", dir)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = register.stdin.take().unwrap();
    stdin
        .write_all(format!("{output}\n{deriver}\n0\n").as_bytes())
        .await
        .unwrap();
    drop(stdin);
    assert!(
        register.wait().await.unwrap().success(),
        "the output is registered"
    );
    output.to_owned()
}

/// Pushes a store path from `store` with the client.
async fn push_with_client(server: &Server, token: &str, dir: &Path, store: &str, path: &str) {
    let output = Command::new(client_bin())
        .args(["nix", "push", "--cache", "nix-main", path])
        .env("GRANARY_SERVER", server.url(""))
        .env("GRANARY_TOKEN", token)
        .env("NIX_REMOTE", store)
        .env("HOME", dir)
        .env("NIX_CONFIG", "experimental-features = nix-command\n")
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "push failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "needs Postgres, object storage and Nix"]
async fn round_trips_with_nix() {
    let stack = Stack::new(CACHES).await;
    stack.write("main.key", MAIN_KEY);
    stack.write("pr.key", PR_KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["nix-main"]).await;
    let dir = stack.dir();
    let source = format!("local?root={}", dir.join("source").display());
    let target = format!("local?root={}", dir.join("target").display());
    let cache = server.url("nix-main");
    stack.write(
        "netrc",
        &format!("machine 127.0.0.1\nlogin granary\npassword {token}\n"),
    );
    let netrc = dir.join("netrc").display().to_string();

    let add = |name: &str| {
        format!(r#"builtins.toFile "{name}" "uses ${{builtins.toFile "{name}-dep" "{name}"}}""#)
    };
    let copied = run_nix(
        &dir,
        &source,
        &["nix", "eval", "--raw", "--expr", &add("copied")],
    )
    .await;
    let pushed = run_nix(
        &dir,
        &source,
        &["nix", "eval", "--raw", "--expr", &add("pushed")],
    )
    .await;
    let xz = run_nix(
        &dir,
        &source,
        &["nix", "eval", "--raw", "--expr", &add("xz")],
    )
    .await;

    run_nix(
        &dir,
        &source,
        &[
            "nix",
            "copy",
            "--to",
            &format!("{cache}?compression=zstd"),
            "--netrc-file",
            &netrc,
            &copied,
        ],
    )
    .await;
    run_nix(
        &dir,
        &source,
        &["nix", "copy", "--to", &cache, "--netrc-file", &netrc, &xz],
    )
    .await;
    push_with_client(&server, &token, &dir, &source, &pushed).await;

    let (_, public_key) = text(&server, "_api/nix/nix-main/public-key").await;
    run_nix(
        &dir,
        &target,
        &[
            "nix",
            "copy",
            "--from",
            &cache,
            "--option",
            "trusted-public-keys",
            &public_key,
            &copied,
            &pushed,
            &xz,
        ],
    )
    .await;
    run_nix(
        &dir,
        &target,
        &[
            "nix",
            "store",
            "verify",
            "--recursive",
            "--trusted-public-keys",
            &public_key,
            &copied,
            &pushed,
            &xz,
        ],
    )
    .await;
    let content = run_nix(&dir, &target, &["nix", "store", "cat", &pushed]).await;
    assert!(
        content.starts_with("uses /nix/store/"),
        "the content arrives: {content}"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}

#[tokio::test]
#[ignore = "needs Postgres, object storage and Nix"]
async fn records_the_system_of_built_paths() {
    let stack = Stack::new(CACHES).await;
    stack.write("main.key", MAIN_KEY);
    stack.write("pr.key", PR_KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["nix-main"]).await;
    let dir = stack.dir();
    let source = format!("local?root={}", dir.join("source").display());

    let built = register_built(&dir, &source).await;
    push_with_client(&server, &token, &dir, &source, &built).await;
    let hash = built
        .trim_start_matches("/nix/store/")
        .split('-')
        .next()
        .unwrap();
    let (_, narinfo) = text(&server, &format!("nix-main/{hash}.narinfo")).await;
    assert_eq!(
        field(&narinfo, "System"),
        "riscv64-linux",
        "the system comes from the deriver: {narinfo}"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}
