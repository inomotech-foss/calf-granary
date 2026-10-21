//! Serving the Yocto hash equivalence protocol.

use std::env;

use tokio::process::Command;

use crate::harness::Stack;

const CACHES: &str = r#"
stores = {}

[caches.hash-main]
protocol = "hashserv"

[caches.hash-pr]
protocol = "hashserv"
fallback = "hash-main"
"#;

#[tokio::test]
#[ignore = "needs Postgres and BitBake"]
async fn round_trips_with_bitbake() {
    let root = env::var("BITBAKE_ROOT").expect("BITBAKE_ROOT locates a BitBake checkout");
    let python = env::var("BITBAKE_PYTHON").unwrap_or_else(|_| "python3".to_owned());
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let main_token = stack.token(&["hash-main"]).await;
    let pr_token = stack.token(&["hash-pr"]).await;
    let url = |cache: &str| {
        server
            .url(&format!("_hashserv/{cache}"))
            .replacen("http", "ws", 1)
    };

    let output = Command::new(python)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/integration/hashserv.py"
        ))
        .args([
            &root,
            &url("hash-main"),
            &url("hash-pr"),
            &main_token,
            &pr_token,
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "the client failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}
