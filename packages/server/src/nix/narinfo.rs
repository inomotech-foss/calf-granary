//! Narinfo files, which describe one store path.

use core::fmt::{self, Write as _};

use super::base32::Base32;
use super::path_info::{PathInfo, parse_sha256};
use super::signing::SigningKey;
use super::store_path::StorePath;
use crate::sha256::Sha256;

/// A narinfo that the cache serves: a store path's metadata and where its
/// NAR is, as one stream of zstd frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NarInfo<'a> {
    /// The store path's metadata.
    pub(crate) path: &'a PathInfo,
    /// The size of the compressed NAR, if known.
    pub(crate) file_size: Option<u64>,
}

impl NarInfo<'_> {
    /// Writes the text that signatures cover.
    pub(crate) fn write_fingerprint<W: fmt::Write>(&self, out: &mut W) -> fmt::Result {
        let path = self.path;
        write!(
            out,
            "1;{};sha256:{};{};",
            path.store_path,
            Base32(path.nar_hash.as_bytes()),
            path.nar_size
        )?;
        for (index, reference) in path.references.iter().enumerate() {
            if index > 0 {
                out.write_char(',')?;
            }
            write!(out, "{reference}")?;
        }
        Ok(())
    }

    /// Appends the narinfo, signed with `key`, to `out`.
    pub(crate) fn render(&self, key: &SigningKey, out: &mut String) -> fmt::Result {
        let path = self.path;
        writeln!(out, "StorePath: {}", path.store_path)?;
        writeln!(out, "URL: nar/{}.nar.zst", path.store_path.hash())?;
        out.write_str("Compression: zstd\n")?;
        if let Some(file_size) = self.file_size {
            writeln!(out, "FileSize: {file_size}")?;
        }
        writeln!(out, "NarHash: sha256:{}", Base32(path.nar_hash.as_bytes()))?;
        writeln!(out, "NarSize: {}", path.nar_size)?;
        out.write_str("References: ")?;
        for (index, reference) in path.references.iter().enumerate() {
            if index > 0 {
                out.write_char(' ')?;
            }
            out.write_str(reference.base_name())?;
        }
        out.write_char('\n')?;
        if let Some(deriver) = &path.deriver {
            writeln!(out, "Deriver: {}", deriver.base_name())?;
        }
        if let Some(system) = &path.system {
            writeln!(out, "System: {system}")?;
        }
        let start = out.len();
        self.write_fingerprint(out)?;
        let signature = key.sign(out.as_bytes().get(start..).unwrap_or_default());
        out.truncate(start);
        writeln!(out, "Sig: {signature}")?;
        if let Some(ca) = &path.ca {
            writeln!(out, "CA: {ca}")?;
        }
        Ok(())
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
    pub(crate) file_hash: Option<Sha256>,
    /// The size of the compressed NAR file.
    pub(crate) file_size: Option<u64>,
}

impl UploadedNarInfo {
    /// Parses the text of a narinfo.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let mut fields = Fields::default();
        for line in text.lines().filter(|line| !line.is_empty()) {
            let (name, value) = line.split_once(": ")?;
            let field = match name {
                "StorePath" => &mut fields.store_path,
                "URL" => &mut fields.url,
                "FileHash" => &mut fields.file_hash,
                "FileSize" => &mut fields.file_size,
                "NarHash" => &mut fields.nar_hash,
                "NarSize" => &mut fields.nar_size,
                "References" => &mut fields.references,
                "Deriver" => &mut fields.deriver,
                "System" => &mut fields.system,
                "CA" => &mut fields.ca,
                _ => continue,
            };
            *field = Some(value);
        }
        let parse_path = |base_name: &str| StorePath::from_base_name(base_name.to_owned()).ok();
        let references = match fields.references.unwrap_or_default() {
            "" => Vec::new(),
            references => references
                .split(' ')
                .map(parse_path)
                .collect::<Option<_>>()?,
        };
        let deriver = match fields.deriver {
            None | Some("unknown-deriver") => None,
            Some(deriver) => Some(parse_path(deriver)?),
        };
        Some(Self {
            path: PathInfo {
                store_path: StorePath::try_from(fields.store_path?.to_owned()).ok()?,
                references,
                deriver,
                system: fields.system.map(str::to_owned),
                ca: fields.ca.map(str::to_owned),
                nar_hash: parse_sha256(fields.nar_hash?)?,
                nar_size: fields.nar_size?.parse().ok()?,
            },
            url: fields.url?.to_owned(),
            file_hash: match fields.file_hash {
                Some(hash) => Some(parse_sha256(hash)?),
                None => None,
            },
            file_size: match fields.file_size {
                Some(size) => Some(size.parse().ok()?),
                None => None,
            },
        })
    }
}

/// The fields of a narinfo that granary reads.
#[derive(Debug, Default)]
struct Fields<'a> {
    /// `StorePath`.
    store_path: Option<&'a str>,
    /// `URL`.
    url: Option<&'a str>,
    /// `FileHash`.
    file_hash: Option<&'a str>,
    /// `FileSize`.
    file_size: Option<&'a str>,
    /// `NarHash`.
    nar_hash: Option<&'a str>,
    /// `NarSize`.
    nar_size: Option<&'a str>,
    /// `References`.
    references: Option<&'a str>,
    /// `Deriver`.
    deriver: Option<&'a str>,
    /// `System`.
    system: Option<&'a str>,
    /// `CA`.
    ca: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nix::signing::tests::TEST_KEY;

    fn path(base_name: &str) -> StorePath {
        StorePath::from_base_name(base_name.to_owned()).unwrap()
    }

    fn path_info() -> PathInfo {
        PathInfo {
            store_path: path("0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12"),
            references: vec![
                path("0mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12"),
                path("1mdqa9w1p6cmli6976v4wi0sw9r4p5pr-glibc-2.40"),
            ],
            deriver: Some(path("2mdqa9w1p6cmli6976v4wi0sw9r4p5pr-hello-2.12.drv")),
            system: Some("x86_64-linux".to_owned()),
            ca: None,
            nar_hash: Sha256::from([0; 32]),
            nar_size: 120,
        }
    }

    #[test]
    fn fingerprints_use_full_paths() {
        let path = path_info();
        let mut fingerprint = String::new();
        NarInfo {
            path: &path,
            file_size: None,
        }
        .write_fingerprint(&mut fingerprint)
        .unwrap();
        assert_eq!(
            fingerprint,
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
        let path = path_info();
        let mut text = String::new();
        NarInfo {
            path: &path,
            file_size: Some(10),
        }
        .render(&key, &mut text)
        .unwrap();
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
        let mut leaf = path_info();
        leaf.references.clear();
        let mut leaf_text = String::new();
        NarInfo {
            path: &leaf,
            file_size: None,
        }
        .render(&key, &mut leaf_text)
        .unwrap();
        assert!(
            leaf_text.contains("\nReferences: \n"),
            "no references still have the separator"
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
