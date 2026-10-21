//! Narinfo files, which describe one store path.

use std::collections::HashMap;

use super::base32;
use super::path_info::{PathInfo, parse_sha256};
use super::signing::SigningKey;
use super::store_path::StorePath;

/// A narinfo: a store path's metadata and where its NAR file is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NarInfo {
    /// The store path's metadata.
    pub(crate) path: PathInfo,
    /// Where the compressed NAR is, relative to the cache.
    pub(crate) url: String,
    /// The compression of the NAR file.
    pub(crate) compression: &'static str,
    /// The size of the compressed NAR file, if known.
    pub(crate) file_size: Option<u64>,
}

impl NarInfo {
    /// The text that signatures cover.
    pub(crate) fn fingerprint(&self) -> String {
        let references: Vec<String> = self
            .path
            .references
            .iter()
            .map(ToString::to_string)
            .collect();
        format!(
            "1;{};sha256:{};{};{}",
            self.path.store_path,
            base32::encode(&self.path.nar_hash),
            self.path.nar_size,
            references.join(",")
        )
    }

    /// Renders the narinfo, signed with `key`.
    pub(crate) fn render(&self, key: &SigningKey) -> String {
        let path = &self.path;
        let references: Vec<String> = path.references.iter().map(StorePath::base_name).collect();
        let mut lines = vec![
            format!("StorePath: {}", path.store_path),
            format!("URL: {}", self.url),
            format!("Compression: {}", self.compression),
        ];
        if let Some(file_size) = self.file_size {
            lines.push(format!("FileSize: {file_size}"));
        }
        lines.push(format!(
            "NarHash: sha256:{}",
            base32::encode(&path.nar_hash)
        ));
        lines.push(format!("NarSize: {}", path.nar_size));
        lines.push(format!("References: {}", references.join(" ")));
        if let Some(deriver) = &path.deriver {
            lines.push(format!("Deriver: {}", deriver.base_name()));
        }
        if let Some(system) = &path.system {
            lines.push(format!("System: {system}"));
        }
        lines.push(format!("Sig: {}", key.sign(&self.fingerprint())));
        if let Some(ca) = &path.ca {
            lines.push(format!("CA: {ca}"));
        }
        lines.push(String::new());
        lines.join("\n")
    }
}

/// A narinfo that a client uploads with `nix copy`.
///
/// Signatures are ignored, because the cache signs with its own key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UploadedNarInfo {
    /// The store path's metadata.
    pub(crate) path: PathInfo,
    /// Where the client uploaded the compressed NAR, relative to the cache.
    pub(crate) url: String,
    /// The SHA-256 of the compressed NAR file.
    pub(crate) file_hash: Option<Vec<u8>>,
    /// The size of the compressed NAR file.
    pub(crate) file_size: Option<u64>,
}

impl UploadedNarInfo {
    /// Parses the text of a narinfo.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut fields = HashMap::new();
        for line in text.lines().filter(|line| !line.is_empty()) {
            let (name, value) = line.split_once(": ")?;
            fields.insert(name, value);
        }
        let parse_path = |base_name: &str| StorePath::from_base_name(base_name).ok();
        let references = match fields.get("References").copied().unwrap_or_default() {
            "" => Vec::new(),
            references => references
                .split(' ')
                .map(parse_path)
                .collect::<Option<_>>()?,
        };
        let deriver = match fields.get("Deriver").copied() {
            None | Some("unknown-deriver") => None,
            Some(deriver) => Some(parse_path(deriver)?),
        };
        Some(Self {
            path: PathInfo {
                store_path: StorePath::try_from(*fields.get("StorePath")?).ok()?,
                references,
                deriver,
                system: fields.get("System").map(ToString::to_string),
                ca: fields.get("CA").map(ToString::to_string),
                nar_hash: parse_sha256(fields.get("NarHash")?)?,
                nar_size: fields.get("NarSize")?.parse().ok()?,
            },
            url: (*fields.get("URL")?).to_owned(),
            file_hash: match fields.get("FileHash") {
                Some(hash) => Some(parse_sha256(hash)?),
                None => None,
            },
            file_size: match fields.get("FileSize") {
                Some(size) => Some(size.parse().ok()?),
                None => None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nix::signing::tests::TEST_KEY;

    fn path(base_name: &str) -> StorePath {
        StorePath::from_base_name(base_name).unwrap()
    }

    fn info() -> NarInfo {
        NarInfo {
            path: PathInfo {
                store_path: path("0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12"),
                references: vec![
                    path("0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12"),
                    path("1mdqa9w1p6cmli6976v4wi0sw9r4p5pr-glibc-2.40"),
                ],
                deriver: Some(path("2mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12.drv")),
                system: Some("x86_64-linux".to_owned()),
                ca: None,
                nar_hash: vec![0; 32],
                nar_size: 120,
            },
            url: "nar/0mdqa9w1p6cmli6976v4wi0sw9r4p5pr.nar.zst".to_owned(),
            compression: "zstd",
            file_size: Some(10),
        }
    }

    #[test]
    fn fingerprints_use_full_paths() {
        assert_eq!(
            info().fingerprint(),
            "1;/nix/store/0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12;sha256:\
             0000000000000000000000000000000000000000000000000000;120;/nix/store/\
             0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12,/nix/store/\
             1mdqa9w1p6cmli6976v4wi0sw9r4p5pr-glibc-2.40",
            "the format that Nix signs"
        );
    }

    #[test]
    fn renders_every_field() {
        let key = SigningKey::load_text(TEST_KEY).unwrap();
        let text = info().render(&key);
        let names: Vec<&str> = text
            .lines()
            .filter_map(|line| line.split_once(": ").map(|(name, _)| name))
            .collect();
        assert_eq!(
            names,
            [
                "StorePath",
                "URL",
                "Compression",
                "FileSize",
                "NarHash",
                "NarSize",
                "References",
                "Deriver",
                "System",
                "Sig"
            ],
            "fields in Nix's order"
        );
        assert!(
            text.contains(
                "References: 0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12 \
                 1mdqa9w1p6cmli6976v4wi0sw9r4p5pr-glibc-2.40\n"
            ),
            "references are base names"
        );
    }

    #[test]
    fn parses_uploaded_narinfos() {
        let text =
            "StorePath: /nix/store/0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12\nURL: \
             nar/1b9p07z77phvv2hf6gm9f28syp39f1ag8jkh6s5rnx7wzpvvf7d6.nar.xz\nCompression: \
             xz\nFileHash: sha256:1b9p07z77phvv2hf6gm9f28syp39f1ag8jkh6s5rnx7wzpvvf7d6\nFileSize: \
             50\nNarHash: sha256:0mdqa9w1p6cmli6976v4wi0sw9r4p5prkj7lzfd1877wk11c9c73\nNarSize: \
             120\nReferences: 0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12\nDeriver: \
             unknown-deriver\nSig: other-1:AAAA\n";
        let info = UploadedNarInfo::parse(text).unwrap();
        assert_eq!(
            info.url, "nar/1b9p07z77phvv2hf6gm9f28syp39f1ag8jkh6s5rnx7wzpvvf7d6.nar.xz",
            "the upload location"
        );
        assert_eq!(info.file_size, Some(50), "the file size");
        assert_eq!(info.path.nar_size, 120, "the NAR size");
        assert_eq!(info.path.references.len(), 1, "the references");
        assert_eq!(info.path.deriver, None, "an unknown deriver");
        assert!(
            UploadedNarInfo::parse("StorePath: /nix/store/x\n").is_none(),
            "required fields"
        );
    }
}
