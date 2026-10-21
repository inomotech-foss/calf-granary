//! Asking the local Nix installation about store paths.

use std::collections::BTreeMap;
use std::io;
use std::process::Stdio;

use serde::Deserialize;
use serde_json::Value;
use tokio::process::{Child, Command};

/// What Nix knows about a store path, as `nix path-info --json` prints it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PathInfo {
    /// The SHA-256 of the NAR.
    pub(crate) nar_hash: String,
    /// The size of the NAR.
    pub(crate) nar_size: u64,
    /// The full store paths that the path references.
    pub(crate) references: Vec<String>,
    /// The derivation that built the path.
    pub(crate) deriver: Option<String>,
    /// The content address of a content-addressed path.
    pub(crate) ca: Option<String>,
}

/// Errors from running Nix.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NixError {
    /// Nix exited with an error.
    #[error("nix failed")]
    Failed,
    /// Nix printed something other than path information.
    #[error("failed to parse the output of nix path-info")]
    Parse(#[source] serde_json::Error),
    /// Nix couldn't be started.
    #[error("failed to run nix")]
    Spawn(#[source] io::Error),
}

/// Returns the store paths and everything they reference, by path.
pub(crate) async fn closure(paths: &[String]) -> Result<BTreeMap<String, PathInfo>, NixError> {
    let output = Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command",
            "path-info",
            "--json",
            "--recursive",
            "--",
        ])
        .args(paths)
        .stderr(Stdio::inherit())
        .output()
        .await
        .map_err(NixError::Spawn)?;
    if !output.status.success() {
        return Err(NixError::Failed);
    }
    serde_json::from_slice(&output.stdout).map_err(NixError::Parse)
}

/// Returns the systems of the derivations that are in the store, by base
/// name.
///
/// Derivations that aren't in the store, such as those of substituted paths,
/// are left out.
pub(crate) async fn systems(derivations: &[String]) -> Result<BTreeMap<String, String>, NixError> {
    if derivations.is_empty() {
        return Ok(BTreeMap::new());
    }
    let invalid = Command::new("nix-store")
        .args(["--check-validity", "--print-invalid", "--"])
        .args(derivations)
        .stderr(Stdio::inherit())
        .output()
        .await
        .map_err(NixError::Spawn)?;
    if !invalid.status.success() {
        return Err(NixError::Failed);
    }
    let invalid = String::from_utf8_lossy(&invalid.stdout);
    let invalid: Vec<&str> = invalid.lines().collect();
    let valid: Vec<&String> = derivations
        .iter()
        .filter(|derivation| !invalid.contains(&derivation.as_str()))
        .collect();
    if valid.is_empty() {
        return Ok(BTreeMap::new());
    }
    let output = Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command",
            "derivation",
            "show",
            "--",
        ])
        .args(valid)
        .stderr(Stdio::inherit())
        .output()
        .await
        .map_err(NixError::Spawn)?;
    if !output.status.success() {
        return Err(NixError::Failed);
    }
    let shown: Value = serde_json::from_slice(&output.stdout).map_err(NixError::Parse)?;
    Ok(derivation_systems(&shown))
}

/// Reads the systems from `nix derivation show` output, by base name.
///
/// Newer Nix nests the derivations under `derivations` and names them by
/// base name, older Nix names them by full path.
fn derivation_systems(shown: &Value) -> BTreeMap<String, String> {
    let derivations = shown.get("derivations").unwrap_or(shown);
    let Some(derivations) = derivations.as_object() else {
        return BTreeMap::new();
    };
    derivations
        .iter()
        .filter_map(|(name, derivation)| {
            let system = derivation.get("system")?.as_str()?;
            let base_name = name.rsplit('/').next()?;
            Some((base_name.to_owned(), system.to_owned()))
        })
        .collect()
}

/// Starts writing the NAR of a store path to the child's stdout.
///
/// This reads through the configured store, so it also works for stores
/// outside `/nix/store`.
pub(crate) fn dump(path: &str) -> Result<Child, NixError> {
    Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command",
            "store",
            "dump-path",
            "--",
            path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(NixError::Spawn)
}

/// The hash part of a store path, which names it in a cache.
pub(crate) fn hash_part(path: &str) -> Option<&str> {
    let (hash, _) = path.rsplit_once('/')?.1.split_once('-')?;
    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_parts() {
        assert_eq!(
            hash_part("/nix/store/mrw721vkndjrsvgs2mpw7nvika924pji-hello-2.12.3"),
            Some("mrw721vkndjrsvgs2mpw7nvika924pji"),
            "the part before the first dash"
        );
        assert_eq!(hash_part("hello"), None, "not a store path");
    }

    #[test]
    fn parses_path_info() {
        let json = r#"{"/nix/store/mrw721vkndjrsvgs2mpw7nvika924pji-hello-2.12.3":{"ca":null,"deriver":"/nix/store/na8zb1nqx3p7hr92n9d8rs6a4kwnqx13-hello-2.12.3.drv","narHash":"sha256-7MZnIH9eNsFHNb8PnARyBCoAG3LUDzeNeskUBTxjZ7A=","narSize":113096,"references":["/nix/store/l6d8k1c03qd9f4vbnn94w8vj7s9m2xsj-libiconv-113"],"registrationTime":1792595339,"signatures":[],"storeDir":"/nix/store","ultimate":false,"version":1}}"#;
        let infos: BTreeMap<String, PathInfo> = serde_json::from_str(json).unwrap();
        let info = infos.values().next().unwrap();
        assert_eq!(info.nar_size, 113_096, "the NAR size");
        assert_eq!(info.references.len(), 1, "the references");
    }

    #[test]
    fn reads_derivation_systems() {
        let newer = serde_json::json!({
            "derivations": { "abc-hello.drv": { "system": "x86_64-linux" } },
            "version": 4,
        });
        let older = serde_json::json!({
            "/nix/store/abc-hello.drv": { "system": "aarch64-linux" },
        });
        assert_eq!(
            derivation_systems(&newer)
                .get("abc-hello.drv")
                .map(String::as_str),
            Some("x86_64-linux"),
            "base names"
        );
        assert_eq!(
            derivation_systems(&older)
                .get("abc-hello.drv")
                .map(String::as_str),
            Some("aarch64-linux"),
            "full paths"
        );
    }
}
