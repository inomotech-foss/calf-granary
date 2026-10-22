//! Applying and verifying migrations.

use crate::harness::{EMPTY, Stack, text};

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn serve_refuses_pending_migrations() {
    let stack = Stack::new(EMPTY).await;
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
async fn refuses_untrusted_database_servers() {
    let stack = Stack::new(EMPTY).await;
    let output = stack
        .command(&["migrate"])
        .env_remove("GRANARY_DATABASE_CA")
        .output()
        .await
        .unwrap();
    assert!(
        !output.status.success(),
        "a server certificate from an unknown CA is refused"
    );
    assert!(
        text(&output).contains("failed to connect"),
        "the connection fails: {}",
        text(&output)
    );
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn migrate_is_idempotent() {
    let stack = Stack::new(EMPTY).await;
    stack.migrate().await;
    let output = stack.run(&["migrate"]).await;
    assert!(output.status.success(), "the second run succeeds");
    assert!(
        text(&output).contains("count=0"),
        "nothing is applied twice: {}",
        text(&output)
    );
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn domains_reject_invalid_values() {
    let stack = Stack::new(EMPTY).await;
    stack.migrate().await;
    let database = stack.database().await;
    let cases = [
        ("'abc'::sha256_digest", "a digest of the wrong length"),
        ("(-1)::byte_count", "a negative size"),
        ("'Main'::cache_name", "an uppercase cache name"),
        ("'a//b'::entry_key", "an empty key segment"),
        ("'../b'::entry_key", "a parent key segment"),
        ("'chunks/xyz'::chunk_key", "a malformed chunk key"),
        (
            "'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee'::nix_hash_part",
            "a letter outside Nix base32",
        ),
        (
            "E'x86_64-linux\\nURL: evil'::nix_info_value",
            "a narinfo value with a line break",
        ),
    ];
    for (value, reason) in cases {
        let result = database.execute(&format!("SELECT {value}"), &[]).await;
        assert!(result.is_err(), "{reason} is rejected");
    }
    let valid = database
        .execute(
            "SELECT 'a/b.tar'::entry_key, '0123456789abcdfghijklmnpqrsvwxyz'::nix_hash_part",
            &[],
        )
        .await;
    assert!(valid.is_ok(), "valid values pass: {valid:?}");
}
