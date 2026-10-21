//! Reading and writing objects over HTTP.

use core::iter;

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH};

use crate::harness::{Server, Stack};

const CACHES: &str = r#"
[caches.main]
store = "test"

[caches.pr]
store = "test"
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
    assert_eq!(
        get(&server, "main/a//b").await.0,
        StatusCode::BAD_REQUEST,
        "invalid keys"
    );
    let pr_token = stack.token(&["pr"]).await;
    put(&server, Some(&pr_token), "pr/only", b"only".to_vec()).await;
    assert_eq!(
        get(&server, "main/only").await.0,
        StatusCode::NOT_FOUND,
        "the fallback is one way"
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
        .query(
            "SELECT cache, key FROM entry WHERE last_access > created_at",
            &[],
        )
        .await
        .unwrap();
    let reads: Vec<(String, String)> = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
    assert_eq!(
        reads,
        [("main".to_owned(), "a".to_owned())],
        "the read is recorded for the entry that served it"
    );
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn stores_identical_content_once() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["main", "pr"]).await;
    for path in ["main/a", "main/b", "pr/c"] {
        assert_eq!(
            put(&server, Some(&token), path, b"same".to_vec()).await,
            StatusCode::CREATED,
            "{path} is stored"
        );
    }
    assert_eq!(
        get(&server, "pr/c").await,
        (StatusCode::OK, b"same".to_vec()),
        "every entry serves the content"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");

    let database = stack.database().await;
    let count = async |table: &str| -> i64 {
        database
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .await
            .unwrap()
            .get(0)
    };
    assert_eq!(count("entry").await, 3, "every key has an entry");
    assert_eq!(count("blob").await, 1, "the content is one blob");
    assert_eq!(count("chunk").await, 1, "the content is one chunk");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn chunks_content_for_deduplication() {
    let stack = Stack::new(
        "[caches.chunked]\nstore = \"test\"\nchunking = true\n\n[caches.whole]\nstore = \"test\"\n",
    )
    .await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["chunked", "whole"]).await;
    let mut state = 7u64;
    let original: Vec<u8> = iter::repeat_with(|| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        state.to_be_bytes()[0]
    })
    .take(3 * 1024 * 1024)
    .collect();
    let mut changed = b"a small change".to_vec();
    changed.extend_from_slice(&original);

    let uploads = [
        ("chunked/original", &original),
        ("chunked/changed", &changed),
        ("whole/original", &original),
    ];
    for (path, content) in uploads {
        assert_eq!(
            put(&server, Some(&token), path, content.clone()).await,
            StatusCode::CREATED,
            "{path} is stored"
        );
        assert_eq!(
            get(&server, path).await,
            (StatusCode::OK, content.clone()),
            "{path} reads back"
        );
    }
    assert!(server.stop().await.success(), "serve stops cleanly");

    let database = stack.database().await;
    let row = database
        .query_one(
            "SELECT count(*), count(*) FILTER (WHERE compression = 'zstd') FROM chunk WHERE \
             EXISTS (SELECT FROM blob_chunk WHERE chunk_id = chunk.id)",
            &[],
        )
        .await
        .unwrap();
    let (chunks, compressed): (i64, i64) = (row.get(0), row.get(1));
    let first: i64 = database
        .query_one(
            "SELECT count(*) FROM blob_chunk JOIN entry USING (blob_id) WHERE entry.key = \
             'original' AND entry.cache = 'chunked'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(first > 10, "the content is split");
    assert_eq!(compressed, chunks, "referenced chunks are compressed");
    assert!(
        chunks <= first + 2,
        "the changed copy shares all but the first chunks: {chunks} for {first}"
    );
    let blobs: i64 = database
        .query_one("SELECT count(*) FROM blob", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(blobs, 2, "the whole copy reuses the chunked blob");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn answers_missing_keys() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["main"]).await;
    put(&server, Some(&token), "main/a/b", b"a".to_vec()).await;
    let missing = async |cache: &str, keys: &[&str]| -> serde_json::Value {
        server
            .http()
            .post(server.url(&format!("_api/files/{cache}/missing")))
            .json(&serde_json::json!({ "keys": keys }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    };
    assert_eq!(
        missing("main", &["a/b", "c"]).await,
        serde_json::json!({ "missing": ["c"] }),
        "only the unknown key is missing"
    );
    assert_eq!(
        missing("pr", &["a/b", "c"]).await,
        serde_json::json!({ "missing": ["c"] }),
        "the fallback counts"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}
