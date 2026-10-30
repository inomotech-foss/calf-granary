//! Pushing Yocto files with the client.

use core::time::Duration;
use std::fs;
use std::path::Path;

use reqwest::StatusCode;
use tokio::process::Command;
use tokio::time;

use crate::harness::{Server, Stack, client_bin, terminate};

const CACHES: &str = r#"
[caches.main]
store = "test"
public = true

[caches.pr]
store = "test"
public = true
fallback = "main"
"#;

const SSTATE: &str =
    "ab/cd/sstate:zlib:core2-64-poky-linux:1.3:r0:core2-64:11:abcd_populate_sysroot.tar.zst";

/// Runs a client command and returns its log.
async fn client(server: &Server, token: &str, args: &[&str]) -> String {
    let output = Command::new(client_bin())
        .args(args)
        .env("GRANARY_SERVER", server.url(""))
        .env("GRANARY_TOKEN", token)
        .output()
        .await
        .unwrap();
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(output.status.success(), "the client failed: {log}");
    log
}

/// Pushes a directory to a cache and returns the client's log.
async fn push(server: &Server, token: &str, command: &str, cache: &str, dir: &Path) -> String {
    let dir = dir.display().to_string();
    client(
        server,
        token,
        &["yocto", command, "--cache", cache, "--dir", &dir],
    )
    .await
}

/// Waits up to 30 seconds for a file to be in the cache.
async fn wait_cached(server: &Server, path: &str) -> bool {
    for _ in 0..300 {
        if status(server, path).await == StatusCode::OK {
            return true;
        }
        time::sleep(Duration::from_millis(100)).await;
    }
    false
}

async fn status(server: &Server, path: &str) -> StatusCode {
    server
        .http()
        .get(server.url(path))
        .send()
        .await
        .unwrap()
        .status()
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn pushes_yocto_files() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let dir = stack.dir();
    let sstate = dir.join("sstate");
    let downloads = dir.join("downloads");
    for (path, content) in [
        (sstate.join(SSTATE), "sstate"),
        (sstate.join(format!("{SSTATE}.siginfo")), "siginfo"),
        (sstate.join(format!("{SSTATE}.Xa8bQ2")), "temporary"),
        (downloads.join("zlib-1.3.1.tar.xz"), "source"),
        (downloads.join("zlib-1.3.1.tar.xz.done"), ""),
        (
            downloads.join("git2/github.com.madler.zlib.git/HEAD"),
            "clone",
        ),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let main_token = stack.token(&["main"]).await;
    let pr_token = stack.token(&["pr"]).await;
    push(&server, &main_token, "push-downloads", "main", &downloads).await;
    let log = push(&server, &pr_token, "push-sstate", "pr", &sstate).await;
    assert!(
        log.contains("files=2 missing=2"),
        "only sstate files count: {log}"
    );
    let again = push(&server, &pr_token, "push-sstate", "pr", &sstate).await;
    assert!(
        again.contains("missing=0"),
        "nothing is uploaded twice: {again}"
    );
    let fallback = push(&server, &pr_token, "push-downloads", "pr", &downloads).await;
    assert!(
        fallback.contains("files=1 missing=0"),
        "the fallback counts: {fallback}"
    );

    let cases = [
        (format!("pr/{SSTATE}"), StatusCode::OK, "sstate is stored"),
        (
            format!("pr/{SSTATE}.siginfo"),
            StatusCode::OK,
            "siginfo is stored",
        ),
        (
            format!("pr/{SSTATE}.Xa8bQ2"),
            StatusCode::NOT_FOUND,
            "temporary files are skipped",
        ),
        (
            "main/zlib-1.3.1.tar.xz".to_owned(),
            StatusCode::OK,
            "sources are stored",
        ),
        (
            "main/zlib-1.3.1.tar.xz.done".to_owned(),
            StatusCode::NOT_FOUND,
            "markers are skipped",
        ),
        (
            "main/git2/github.com.madler.zlib.git/HEAD".to_owned(),
            StatusCode::NOT_FOUND,
            "clones are skipped",
        ),
    ];
    for (path, expected, reason) in cases {
        assert_eq!(status(&server, &path).await, expected, "{reason}");
    }
    assert!(server.stop().await.success(), "serve stops cleanly");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn uploads_while_watching() {
    let stack = Stack::new(CACHES).await;
    stack.migrate().await;
    let server = stack.serve().await;
    let sstate = stack.dir().join("sstate");
    fs::create_dir_all(&sstate).unwrap();
    let token = stack.token(&["pr"]).await;
    let mut watcher = Command::new(client_bin())
        .args(["watch", "--interval", "1", "--settle", "0"])
        .args(["--sstate-cache", "pr", "--sstate-dir"])
        .arg(&sstate)
        .env("GRANARY_SERVER", server.url(""))
        .env("GRANARY_TOKEN", &token)
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let first = sstate.join(SSTATE);
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::write(&first, "sstate").unwrap();
    assert!(
        wait_cached(&server, &format!("pr/{SSTATE}")).await,
        "new files are uploaded"
    );
    let second = SSTATE.replace("ab/cd/", "ef/01/");
    fs::create_dir_all(sstate.join("ef/01")).unwrap();
    fs::write(sstate.join(&second), "sstate").unwrap();
    assert!(
        wait_cached(&server, &format!("pr/{second}")).await,
        "files in new directories are uploaded"
    );
    terminate(&mut watcher).await;
    assert!(server.stop().await.success(), "serve stops cleanly");
}
