//! Serving Nix binary caches.

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::signature::{ED25519, UnparsedPublicKey};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use serde_json::{Value, json};

use crate::harness::{Server, Stack};

const CACHES: &str = r#"
[caches.nix-main]
store = "test"
protocol = "nix"
signing_key = "$dir/main.key"

[caches.nix-pr]
store = "test"
protocol = "nix"
signing_key = "$dir/pr.key"
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
        .header("x-granary-nar-info", info.to_string())
        .body(nar);
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
            StatusCode::UNAUTHORIZED,
            "uploads need a token",
        ),
        (
            Some(token.as_str()),
            &wrong,
            StatusCode::BAD_REQUEST,
            "the hash must match",
        ),
        (
            Some(token.as_str()),
            &info,
            StatusCode::CREATED,
            "a new path is stored",
        ),
        (
            Some(token.as_str()),
            &info,
            StatusCode::NO_CONTENT,
            "an existing path is kept",
        ),
    ];
    for (bearer, metadata, expected, reason) in cases {
        assert_eq!(
            upload(&server, bearer, metadata, nar.clone()).await,
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
