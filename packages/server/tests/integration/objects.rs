//! Reading and writing objects over HTTP.

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH};

use crate::harness::{Server, Stack};

const CACHES: &str = r#"
[caches.main]
bucket = "$bucket"
prefix = "$prefix/main"

[caches.pr]
bucket = "$bucket"
prefix = "$prefix/pr"
fallback = "main"
"#;

async fn put(server: &Server, token: Option<&str>, path: &str, body: Vec<u8>) -> StatusCode {
    let mut request = server.http().put(server.url(path)).body(body);
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    request.send().await.unwrap().status()
}

async fn get(server: &Server, path: &str) -> (StatusCode, Vec<u8>) {
    let response = server.http().get(server.url(path)).send().await.unwrap();
    let status = response.status();
    (status, response.bytes().await.unwrap().to_vec())
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn uploads_and_reads_objects() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["main"]).await;
    let content = b"content".to_vec();

    let cases = [
        (
            Some(token.as_str()),
            "main/a/b",
            StatusCode::CREATED,
            "a new object is stored",
        ),
        (
            Some(token.as_str()),
            "main/a/b",
            StatusCode::NO_CONTENT,
            "an existing object is kept",
        ),
        (
            Some(token.as_str()),
            "pr/a/b",
            StatusCode::FORBIDDEN,
            "other caches are refused",
        ),
        (
            None,
            "main/c",
            StatusCode::UNAUTHORIZED,
            "writes need a token",
        ),
        (
            Some("invalid"),
            "main/c",
            StatusCode::UNAUTHORIZED,
            "the token must be valid",
        ),
    ];
    for (bearer, path, expected, reason) in cases {
        assert_eq!(
            put(&server, bearer, path, content.clone()).await,
            expected,
            "{reason}"
        );
    }

    assert_eq!(
        get(&server, "main/a/b").await,
        (StatusCode::OK, content.clone()),
        "the object is served"
    );
    assert_eq!(
        get(&server, "pr/a/b").await,
        (StatusCode::OK, content),
        "the fallback serves objects the cache lacks"
    );
    assert_eq!(
        get(&server, "main/missing").await.0,
        StatusCode::NOT_FOUND,
        "missing objects"
    );
    assert_eq!(
        get(&server, "other/a/b").await.0,
        StatusCode::NOT_FOUND,
        "unknown caches"
    );
    let head = server
        .http()
        .head(server.url("main/a/b"))
        .send()
        .await
        .unwrap();
    assert_eq!(head.headers()[CONTENT_LENGTH], "7", "HEAD reports the size");

    assert!(server.stop().await.success(), "serve stops cleanly");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn uploads_large_objects_in_parts() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["main"]).await;
    let content: Vec<u8> = (0..20 * 1024 * 1024u32)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();

    assert_eq!(
        put(&server, Some(&token), "main/large", content.clone()).await,
        StatusCode::CREATED,
        "the object is stored"
    );
    assert_eq!(
        get(&server, "main/large").await,
        (StatusCode::OK, content),
        "the parts are joined in order"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn records_reads_on_shutdown() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["main"]).await;
    put(&server, Some(&token), "main/a", b"a".to_vec()).await;
    assert_eq!(
        get(&server, "pr/a").await.0,
        StatusCode::OK,
        "the fallback serves it"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");

    let rows = stack
        .database()
        .await
        .query("SELECT bucket, key FROM access", &[])
        .await
        .unwrap();
    let reads: Vec<(String, String)> = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
    assert_eq!(
        reads,
        [(stack.bucket().to_owned(), stack.key("main/a"))],
        "the read is recorded for the object that served it"
    );
}
