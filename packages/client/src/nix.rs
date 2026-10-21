//! Asking the local Nix installation about store paths.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::process::Stdio;

use serde::Deserialize;
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
    /// Nix printed something unexpected.
    #[error("failed to parse the output of nix")]
    Parse(#[source] serde_json::Error),
    /// Nix couldn't be started.
    #[error("failed to run nix")]
    Spawn(#[source] io::Error),
}

/// The systems of derivations, as `nix derivation show` prints them.
#[derive(Debug, Default, Deserialize)]
#[serde(untagged)]
pub(crate) enum Systems {
    /// No derivations were shown.
    #[default]
    None,
    /// Newer Nix nests the derivations and names them by base name.
    Nested {
        /// The derivations by base name.
        derivations: BTreeMap<String, Derivation>,
    },
    /// Older Nix names the derivations by full path.
    Flat(BTreeMap<String, Derivation>),
}

/// The part of a derivation that matters for pushing.
#[derive(Debug, Deserialize)]
pub(crate) struct Derivation {
    /// The system that builds it.
    system: String,
}

impl Systems {
    /// Reads the systems of the derivations that are in the store.
    ///
    /// Derivations that aren't in the store, such as those of substituted
    /// paths, are left out.
    pub(crate) async fn read(derivations: &[&str]) -> Result<Self, NixError> {
        if derivations.is_empty() {
            return Ok(Self::None);
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
        let invalid: HashSet<&str> = invalid.lines().collect();
        let mut valid = derivations
            .iter()
            .filter(|derivation| !invalid.contains(*derivation))
            .peekable();
        if valid.peek().is_none() {
            return Ok(Self::None);
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
        serde_json::from_slice(&output.stdout).map_err(NixError::Parse)
    }

    /// The system of a derivation, given as a full store path.
    pub(crate) fn get(&self, deriver: &str) -> Option<&str> {
        let derivation = match self {
            Self::None => None,
            Self::Nested { derivations } => derivations.get(deriver.rsplit('/').next()?),
            Self::Flat(derivations) => derivations.get(deriver),
        }?;
        Some(&derivation.system)
    }
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
        let deriver = "/nix/store/abc-hello.drv";
        let newer: Systems = serde_json::from_str(
            r#"{"derivations": {"abc-hello.drv": {"system": "x86_64-linux"}}, "version": 4}"#,
        )
        .unwrap();
        let older: Systems =
            serde_json::from_str(r#"{"/nix/store/abc-hello.drv": {"system": "aarch64-linux"}}"#)
                .unwrap();
        assert_eq!(newer.get(deriver), Some("x86_64-linux"), "base names");
        assert_eq!(older.get(deriver), Some("aarch64-linux"), "full paths");
    }
}
