//! Deleting objects that outlived their retention.

use core::time::Duration;

use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use tokio::time;

use crate::harness::{Stack, text};

const CACHES: &str = r#"
[caches.short]
bucket = "$bucket"
prefix = "$prefix/short"
retention = "3s"

[caches.kept]
bucket = "$bucket"
prefix = "$prefix/kept"
"#;

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn prunes_unused_objects() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let token = stack.token(&["short", "kept"]).await;
    for path in ["short/read", "short/unread", "kept/unread"] {
        let status = server
            .http()
            .put(server.url(path))
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(path.as_bytes().to_vec())
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::CREATED, "{path} is stored");
    }
    time::sleep(Duration::from_secs(4)).await;
    let read = server
        .http()
        .get(server.url("short/read"))
        .send()
        .await
        .unwrap();
    assert_eq!(read.status(), StatusCode::OK, "the object is read");
    assert!(server.stop().await.success(), "serve records the read");

    let output = stack.run(&["prune"]).await;
    assert!(output.status.success(), "prune failed: {}", text(&output));

    let restarted = stack.serve().await;
    let cases = [
        (
            "short/read",
            StatusCode::OK,
            "a recently read object is kept",
        ),
        (
            "short/unread",
            StatusCode::NOT_FOUND,
            "an unused object is deleted",
        ),
        (
            "kept/unread",
            StatusCode::OK,
            "caches without retention are kept",
        ),
    ];
    for (path, expected, reason) in cases {
        let status = restarted
            .http()
            .head(restarted.url(path))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, expected, "{reason}");
    }
    assert!(restarted.stop().await.success(), "serve stops cleanly");

    let runs = stack
        .database()
        .await
        .query("SELECT cache, deleted FROM prune_run", &[])
        .await
        .unwrap();
    let runs: Vec<(String, i64)> = runs.iter().map(|row| (row.get(0), row.get(1))).collect();
    assert_eq!(runs, [("short".to_owned(), 1)], "the run is recorded");
}
