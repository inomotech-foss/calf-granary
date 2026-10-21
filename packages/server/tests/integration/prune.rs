//! Deleting objects that outlived their retention.

use core::time::Duration;

use object_store::path::Path;
use object_store::{ObjectStoreExt as _, PutPayload};
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use tokio::time;

use crate::harness::{Stack, text};

const CACHES: &str = r#"
collection_grace = "1s"

[caches.short]
store = "test"
retention = "3s"

[caches.kept]
store = "test"
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

    let database = stack.database().await;
    let expiries = database
        .query("SELECT cache, expired FROM prune_expiry", &[])
        .await
        .unwrap();
    let expiries: Vec<(String, i64)> = expiries
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();
    assert_eq!(
        expiries,
        [("short".to_owned(), 1)],
        "the expiry is recorded"
    );
    let collection = database
        .query_one(
            "SELECT deleted_blobs, deleted_chunks FROM prune_collection",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        (collection.get::<_, i64>(0), collection.get::<_, i64>(1)),
        (1, 1),
        "the content of the expired entry is collected"
    );
    let chunks: i64 = database
        .query_one("SELECT count(*) FROM chunk", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(chunks, 2, "the content of the kept entries remains");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn sweeps_objects_without_chunks() {
    let stack = Stack::new("collection_grace = \"1s\"\ncaches = {}").await;
    stack.migrate().await;
    let store = stack.store();
    let orphan = Path::from("chunks/orphan");
    store
        .put(&orphan, PutPayload::from("orphan"))
        .await
        .unwrap();
    time::sleep(Duration::from_secs(2)).await;

    let output = stack.run(&["prune"]).await;
    assert!(output.status.success(), "prune failed: {}", text(&output));
    assert!(
        matches!(
            store.head(&orphan).await,
            Err(object_store::Error::NotFound { path: _, source: _ })
        ),
        "the object is deleted"
    );
    let swept: i64 = stack
        .database()
        .await
        .query_one("SELECT swept_objects FROM prune_collection", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(swept, 1, "the sweep is recorded");
}
