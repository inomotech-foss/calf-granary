//! Applying and verifying migrations.

use crate::harness::{Stack, text};

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn serve_refuses_pending_migrations() {
    let stack = Stack::new("caches = {}").await;
    let output = stack.run(&["serve"]).await;
    assert!(!output.status.success(), "serve refuses to start");
    assert!(
        text(&output).contains("pending migrations"),
        "the error names the cause: {}",
        text(&output)
    );
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn migrate_is_idempotent() {
    let stack = Stack::new("caches = {}").await;
    stack.migrate().await;
    let output = stack.run(&["migrate"]).await;
    assert!(output.status.success(), "the second run succeeds");
    assert!(
        text(&output).contains("count=0"),
        "nothing is applied twice: {}",
        text(&output)
    );
}
