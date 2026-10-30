//! Asking the local Nix installation about store paths.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, btree_map};
use std::io;
use std::process::Stdio;

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt as _, BufReader};
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
    /// The output of Nix can't be read.
    #[error("failed to read the output of nix")]
    Read(#[source] io::Error),
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
    pub(crate) async fn read(derivations: &BTreeSet<&str>) -> Result<Self, NixError> {
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

/// Store paths and what Nix knows about them, by path.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct Closure(BTreeMap<String, PathInfo>);

impl Closure {
    /// Returns the store paths and everything they reference.
    pub(crate) async fn of(paths: &[String]) -> Result<Self, NixError> {
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

    /// The number of store paths.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// The store paths and their info.
    pub(crate) fn iter(&self) -> btree_map::Iter<'_, String, PathInfo> {
        self.0.iter()
    }

    /// Groups the selected paths into rounds. Each path comes after the
    /// selected paths that it references, so a cache never holds a path
    /// without its references.
    pub(crate) fn rounds(&self, selected: impl Fn(&str) -> bool) -> Vec<Vec<(&str, &PathInfo)>> {
        let mut waiting: HashMap<&str, usize> = HashMap::new();
        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for (path, info) in self.iter().filter(|(path, _)| selected(path)) {
            let references = info
                .references
                .iter()
                .filter(|reference| *reference != path && selected(reference))
                .filter(|reference| self.0.contains_key(*reference));
            let mut count = 0usize;
            for reference in references {
                dependents.entry(reference).or_default().push(path);
                count = count.saturating_add(1);
            }
            waiting.insert(path, count);
        }
        let mut round: Vec<&str> = waiting
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(path, _)| *path)
            .collect();
        let mut rounds = Vec::new();
        while !round.is_empty() {
            let mut next = Vec::new();
            for path in &round {
                for dependent in dependents.get(path).into_iter().flatten() {
                    if let Some(count) = waiting.get_mut(dependent) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            next.push(*dependent);
                        }
                    }
                }
            }
            rounds.push(
                round
                    .into_iter()
                    .filter_map(|path| self.0.get_key_value(path))
                    .map(|(path, info)| (path.as_str(), info))
                    .collect(),
            );
            round = next;
        }
        rounds
    }
}

/// Calls `found` with every valid path in the store, while Nix lists them.
pub(crate) async fn each_path<F>(mut found: F) -> Result<(), NixError>
where
    F: FnMut(&str),
{
    let mut child = Command::new("nix")
        .args([
            "--extra-experimental-features",
            "nix-command",
            "path-info",
            "--all",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(NixError::Spawn)?;
    let stdout = child.stdout.take().ok_or(NixError::Failed)?;
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    while reader.read_line(&mut line).await.map_err(NixError::Read)? != 0 {
        found(line.trim_end());
        line.clear();
    }
    if !child.wait().await.map_err(NixError::Spawn)?.success() {
        return Err(NixError::Failed);
    }
    Ok(())
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
    fn uploads_references_first() {
        let info = |references: &[&str]| {
            serde_json::json!({
                "narHash": "sha256-7MZnIH9eNsFHNb8PnARyBCoAG3LUDzeNeskUBTxjZ7A=",
                "narSize": 1,
                "references": references,
            })
        };
        let closure: Closure = serde_json::from_value(serde_json::json!({
            "app": info(&["app", "lib", "libc"]),
            "lib": info(&["libc"]),
            "libc": info(&[]),
            "cached": info(&["libc"]),
        }))
        .unwrap();
        let rounds: Vec<Vec<&str>> = closure
            .rounds(|path| path != "cached")
            .into_iter()
            .map(|round| round.into_iter().map(|(path, _)| path).collect())
            .collect();
        assert_eq!(
            rounds,
            [vec!["libc"], vec!["lib"], vec!["app"]],
            "references come first, unselected paths are left out"
        );
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
