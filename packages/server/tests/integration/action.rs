//! The CI action against a real server, a fake forge and a local Nix store.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

use reqwest::StatusCode;
use tokio::process::Command;

use crate::forge::{Forge, RUNNER_TOKEN};
use crate::harness::{Server, Stack, client_bin, manifest_dir};

const KEY: &str =
    "pr-1:gDQElRwG6hWQbzvWiPbOwXBTfsbJksJCSpVX/RYucmv/Qjzzg/1fQ4UimSnxIDFlz+sk9nb4Z1Ws41Rc6LhzIg==";

const SSTATE: &str =
    "ab/cd/sstate:zlib:core2-64-poky-linux:1.3:r0:core2-64:11:abcd_populate_sysroot.tar.zst";

/// Reads `name<<delimiter` blocks from a runner file.
fn file_commands(path: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut commands = BTreeMap::new();
    let mut lines = text.lines();
    while let Some(header) = lines.next() {
        let Some((name, delimiter)) = header.split_once("<<") else {
            continue;
        };
        let value: Vec<&str> = lines
            .by_ref()
            .take_while(|line| *line != delimiter)
            .collect();
        commands.insert(name.to_owned(), value.join("\n"));
    }
    commands
}

/// Runs a step of the action and returns its log.
async fn step(script: &str, envs: &BTreeMap<String, String>) -> String {
    let node = env::var("GRANARY_NODE").unwrap_or_else(|_| "node".to_owned());
    let action = manifest_dir().join("../../action");
    let output = Command::new(node)
        .arg(action.join(script))
        .envs(envs)
        .output()
        .await
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() && !log.contains("::error::"),
        "{script} failed: {log}"
    );
    log
}

/// Adds a store path with a reference to the store and returns it.
async fn add_store_path(envs: &BTreeMap<String, String>, name: &str) -> String {
    let expression =
        format!(r#"builtins.toFile "{name}" "uses ${{builtins.toFile "{name}-dep" "{name}"}}""#);
    let output = Command::new("nix")
        .args(["eval", "--raw", "--expr", &expression])
        .envs(envs)
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// A server configuration with every kind of cache and a tier for pull
/// requests from the forge.
fn config(forge: &str) -> String {
    format!(
        r#"
[caches.sstate-pr]
store = "test"
role = "sstate"
public = true

[caches.downloads]
store = "test"
role = "downloads"

[caches.hash-pr]
protocol = "hashserv"
public = true

[caches.nix-pr]
store = "test"
protocol = "nix"
signing_key = "$dir/pr.key"
public = true

[issuers.forge]
issuer = "{forge}"
audience = "calf-granary"
claims = {{ repository_owner = ["inomotech"] }}

[[tiers]]
name = "pr"
writes = ["sstate-pr", "downloads", "hash-pr", "nix-pr"]
claims = {{ repository_owner = ["inomotech"] }}
"#
    )
}

/// The environment that a runner gives the action's steps.
fn runner_env(server: &str, forge: &str, dir: &Path) -> BTreeMap<String, String> {
    let show = |name: &str| dir.join(name).display().to_string();
    [
        ("INPUT_SERVER", server.to_owned()),
        ("INPUT_AUDIENCE", "calf-granary".to_owned()),
        ("INPUT_NIX", "true".to_owned()),
        ("INPUT_BITBAKE-CONF", show("conf/auto.conf")),
        ("INPUT_SSTATE-DIR", show("sstate")),
        ("INPUT_DOWNLOADS-DIR", show("downloads")),
        ("INPUT_CLIENT", client_bin().display().to_string()),
        ("RUNNER_TEMP", show("runner")),
        ("GITHUB_ENV", show("env")),
        ("GITHUB_STATE", show("state")),
        ("GITHUB_OUTPUT", show("output")),
        (
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            format!("{forge}/token?api-version=2.0"),
        ),
        ("ACTIONS_ID_TOKEN_REQUEST_TOKEN", RUNNER_TOKEN.to_owned()),
        ("NIX_REMOTE", format!("local?root={}", show("store"))),
        (
            "NIX_CONFIG",
            "experimental-features = nix-command".to_owned(),
        ),
        ("HOME", show("")),
        ("PATH", env::var("PATH").unwrap_or_default()),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect()
}

/// Checks that post uploaded what the job built and nothing older.
async fn assert_uploaded(server: &Server, token: &str, before: &str, built: &str) {
    let hash = |store_path: &str| {
        store_path
            .trim_start_matches("/nix/store/")
            .split('-')
            .next()
            .unwrap()
            .to_owned()
    };
    let cases = [
        (
            format!("sstate-pr/{SSTATE}"),
            StatusCode::OK,
            "sstate is uploaded",
        ),
        (
            "downloads/zlib-1.3.1.tar.xz".to_owned(),
            StatusCode::OK,
            "downloads are uploaded",
        ),
        (
            format!("nix-pr/{}.narinfo", hash(built)),
            StatusCode::OK,
            "new store paths are pushed",
        ),
        (
            format!("nix-pr/{}.narinfo", hash(before)),
            StatusCode::NOT_FOUND,
            "old store paths aren't",
        ),
    ];
    for (url, expected, reason) in cases {
        let status = server
            .http()
            .get(server.url(&url))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, expected, "{reason}");
    }
}

#[tokio::test]
#[ignore = "needs Postgres, object storage, Nix and Node"]
async fn sets_up_and_fills_caches() {
    let forge = Forge::start().await;
    let config = config(forge.url());
    let stack = Stack::new(&config).await;
    stack.write("pr.key", KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let dir = stack.dir();
    let path = |name: &str| -> PathBuf { dir.join(name) };
    for name in ["runner", "sstate", "downloads", "conf"] {
        fs::create_dir_all(path(name)).unwrap();
    }
    for name in ["env", "state", "output"] {
        fs::write(path(name), "").unwrap();
    }
    let mut envs = runner_env(&server.url(""), forge.url(), &dir);
    let before = add_store_path(&envs, "before").await;

    step("main.ts", &envs).await;
    let outputs = file_commands(&path("output"));
    assert_eq!(
        outputs.get("tier").map(String::as_str),
        Some("pr"),
        "the tier is an output"
    );
    let exported = file_commands(&path("env"));
    let nix_config = exported.get("NIX_CONFIG").cloned().unwrap_or_default();
    assert!(
        nix_config.contains(&server.url("nix-pr")),
        "Nix substitutes: {nix_config}"
    );
    let bitbake = fs::read_to_string(path("conf/auto.conf")).unwrap();
    for setting in ["SSTATE_MIRRORS", "PREMIRRORS", "BB_HASHSERVE = \"ws://"] {
        assert!(
            bitbake.contains(setting),
            "BitBake gets {setting}: {bitbake}"
        );
    }
    assert!(
        !bitbake.contains("PASSWORD"),
        "the token stays out of the build directory: {bitbake}"
    );
    let home_netrc = fs::read_to_string(path(".netrc")).unwrap();
    assert!(
        home_netrc.contains(&format!(
            "machine {}",
            server.url("_hashserv/hash-pr").replacen("http", "ws", 1)
        )),
        "BitBake finds the token in netrc: {home_netrc}"
    );
    assert!(
        home_netrc.contains("machine 127.0.0.1"),
        "BitBake's fetcher finds the token in netrc: {home_netrc}"
    );

    let built = add_store_path(&envs, "built").await;
    fs::create_dir_all(path("sstate").join(SSTATE).parent().unwrap()).unwrap();
    fs::write(path("sstate").join(SSTATE), "sstate").unwrap();
    fs::write(path("downloads/zlib-1.3.1.tar.xz"), "source").unwrap();
    for (name, value) in file_commands(&path("state")) {
        envs.insert(format!("STATE_{name}"), value);
    }
    envs.insert(
        "NIX_CONFIG".to_owned(),
        format!("{nix_config}\nexperimental-features = nix-command"),
    );
    let log = step("post.ts", &envs).await;
    assert!(!log.contains("::warning::"), "uploads succeed: {log}");
    assert!(
        !path(".netrc").exists() && !path("runner/granary.netrc").exists(),
        "the credentials are removed"
    );

    let reader = stack
        .read_token(&["sstate-pr", "downloads", "nix-pr"])
        .await;
    assert_uploaded(&server, &reader, &before, &built).await;
    assert!(server.stop().await.success(), "serve stops cleanly");
}

#[tokio::test]
#[ignore = "needs Postgres, object storage, Nix and Node"]
async fn reads_public_caches_without_id_tokens() {
    let forge = Forge::start().await;
    let stack = Stack::new(&config(forge.url())).await;
    stack.write("pr.key", KEY);
    stack.migrate().await;
    let server = stack.serve().await;
    let dir = stack.dir();
    for name in ["runner", "sstate", "downloads", "conf"] {
        fs::create_dir_all(dir.join(name)).unwrap();
    }
    for name in ["env", "state", "output"] {
        fs::write(dir.join(name), "").unwrap();
    }
    let mut envs = runner_env(&server.url(""), forge.url(), &dir);
    envs.remove("ACTIONS_ID_TOKEN_REQUEST_URL");
    envs.remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN");

    let log = step("main.ts", &envs).await;
    assert!(
        log.contains("reading the public caches"),
        "the job is warned: {log}"
    );
    let exported = file_commands(&dir.join("env"));
    let nix_config = exported.get("NIX_CONFIG").cloned().unwrap_or_default();
    assert!(
        nix_config.contains(&server.url("nix-pr")),
        "Nix substitutes: {nix_config}"
    );
    assert!(
        !nix_config.contains("netrc-file"),
        "without credentials: {nix_config}"
    );
    let bitbake = fs::read_to_string(dir.join("conf/auto.conf")).unwrap();
    assert!(
        bitbake.contains("SSTATE_MIRRORS"),
        "BitBake reads sstate: {bitbake}"
    );
    assert!(
        !bitbake.contains("PREMIRRORS"),
        "downloads aren't public here: {bitbake}"
    );
    assert!(
        !bitbake.contains("PASSWORD"),
        "without credentials: {bitbake}"
    );

    let post = step("post.ts", &envs).await;
    assert!(
        post.contains("nothing to upload"),
        "nothing is uploaded: {post}"
    );
    assert!(server.stop().await.success(), "serve stops cleanly");
}
