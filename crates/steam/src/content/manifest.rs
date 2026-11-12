//! Depot manifests: a zip with one entry `z`, holding `[u32 magic][u32 len][protobuf]` sections.

use std::io::Read;

use base64::Engine;
use prost::Message;

use super::crypto::{self, Key};
use crate::proto::{ContentManifestMetadata, ContentManifestPayload};
use crate::{Error, Result};

const PAYLOAD: u32 = 0x71F6_17D0;
const METADATA: u32 = 0x1F48_12BE;
const SIGNATURE: u32 = 0x1B81_B817;
const END: u32 = 0x32C4_15AB;
const DIRECTORY: u32 = 64;
/// Steam chunks are at most 1 MiB; anything bigger is a broken or hostile manifest.
const CHUNK_CAP: u32 = 16 << 20;
/// Manifests of full game depots are a few MB; this only stops garbage.
const CAP: u64 = 256 << 20;

#[derive(Debug, Clone)]
pub struct Manifest {
    pub depot: u32,
    pub id: u64,
    pub files: Vec<File>,
}

#[derive(Debug, Clone)]
pub struct File {
    /// Relative path with `/` separators.
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub sha: Vec<u8>,
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub sha: Vec<u8>,
    pub crc: u32,
    pub offset: u64,
    pub original: u32,
    pub compressed: u32,
}

impl Chunk {
    /// The CDN names chunks by their SHA-1 in hex.
    pub fn id(&self) -> String {
        self.sha.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Parses a manifest download. `key` decrypts file names when the manifest says they are encrypted.
pub fn parse(zip: &[u8], key: &Key) -> Result<Manifest> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|_| Error::Packet("manifest is not a zip"))?;
    let mut z = Vec::new();
    archive
        .by_name("z")
        .map_err(|_| Error::Packet("manifest zip has no 'z' entry"))?
        .take(CAP)
        .read_to_end(&mut z)?;

    let (mut payload, mut meta) = (None, None);
    let mut at = 0;
    loop {
        let word = |at: usize| -> Result<u32> {
            let raw = z
                .get(at..at + 4)
                .ok_or(Error::Packet("manifest truncated"))?;
            Ok(u32::from_le_bytes(raw.try_into().expect("4 bytes")))
        };
        let magic = word(at)?;
        if magic == END {
            break;
        }
        let len = word(at + 4)? as usize;
        let body = z
            .get(at + 8..at + 8 + len)
            .ok_or(Error::Packet("manifest section past end"))?;
        match magic {
            PAYLOAD => payload = Some(ContentManifestPayload::decode(body)?),
            METADATA => meta = Some(ContentManifestMetadata::decode(body)?),
            SIGNATURE => {}
            _ => return Err(Error::Packet("unknown manifest section")),
        }
        at += 8 + len;
    }
    let payload = payload.ok_or(Error::Packet("manifest without payload"))?;
    let meta = meta.ok_or(Error::Packet("manifest without metadata"))?;

    let mut files = Vec::with_capacity(payload.mappings.len());
    for m in payload.mappings {
        let mut name = m.filename.unwrap_or_default();
        if meta.filenames_encrypted() {
            // Steam wraps long names in newlines; drop whitespace before decoding.
            let compact: String = name.chars().filter(|c| !c.is_whitespace()).collect();
            let raw = base64::engine::general_purpose::STANDARD
                .decode(compact)
                .map_err(|_| Error::Packet("encrypted file name is not base64"))?;
            let plain = crypto::decrypt(key, &raw)?;
            name = String::from_utf8_lossy(&plain)
                .trim_end_matches('\0')
                .to_owned();
        }
        let mut chunks: Vec<Chunk> = m
            .chunks
            .into_iter()
            .map(|c| Chunk {
                sha: c.sha.unwrap_or_default(),
                crc: c.crc.unwrap_or(0),
                offset: c.offset.unwrap_or(0),
                original: c.cb_original.unwrap_or(0),
                compressed: c.cb_compressed.unwrap_or(0),
            })
            .collect();
        if chunks
            .iter()
            .any(|c| c.original > CHUNK_CAP || c.compressed > CHUNK_CAP)
        {
            return Err(Error::Packet("chunk larger than Steam ever makes"));
        }
        chunks.sort_by_key(|c| c.offset);
        files.push(File {
            path: safe_path(&name)?,
            size: m.size.unwrap_or(0),
            is_dir: m.flags.unwrap_or(0) & DIRECTORY != 0,
            sha: m.sha_content.unwrap_or_default(),
            chunks,
        });
    }
    Ok(Manifest {
        depot: meta.depot_id.unwrap_or(0),
        id: meta.gid_manifest.unwrap_or(0),
        files,
    })
}

/// Normalizes `\\` to `/` and accepts only plain relative components, so a manifest can't write
/// outside the install folder on any OS.
fn safe_path(name: &str) -> Result<String> {
    use std::path::{Component, Path};

    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let path = name.replace('\\', "/");
    let parts_ok = !path.is_empty()
        && path.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or(part);
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
                && !RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem))
        });
    let components_ok = Path::new(&path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if !parts_ok || !components_ok {
        return Err(Error::Packet("unsafe path in manifest"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::content::crypto::tests::encrypt;
    use crate::proto::{ChunkData, FileMapping};

    fn section(out: &mut Vec<u8>, magic: u32, body: &impl Message) {
        let body = body.encode_to_vec();
        out.extend_from_slice(&magic.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
    }

    fn manifest_zip(names: &[(&str, u32)], key: &Key) -> Vec<u8> {
        let payload = ContentManifestPayload {
            mappings: names
                .iter()
                .map(|(name, flags)| FileMapping {
                    // Wrapped like Steam does, to check whitespace is ignored.
                    filename: Some({
                        let b64 = base64::engine::general_purpose::STANDARD.encode(encrypt(
                            key,
                            [5u8; 16],
                            format!("{name}\0").as_bytes(),
                        ));
                        format!("{}\n{}", &b64[..8], &b64[8..])
                    }),
                    flags: Some(*flags),
                    size: Some(3),
                    chunks: vec![
                        ChunkData {
                            offset: Some(2),
                            ..Default::default()
                        },
                        ChunkData {
                            offset: Some(0),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                })
                .collect(),
        };
        let meta = ContentManifestMetadata {
            depot_id: Some(108600),
            gid_manifest: Some(42),
            filenames_encrypted: Some(true),
        };
        let mut z = Vec::new();
        section(&mut z, PAYLOAD, &payload);
        section(&mut z, METADATA, &meta);
        z.extend_from_slice(&END.to_le_bytes());

        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("z", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&z).unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn parses_encrypted_names_dirs_and_sorts_chunks() {
        let key = [4u8; 32];
        let zip = manifest_zip(
            &[("mods\\Example", DIRECTORY), ("mods\\Example\\mod.info", 0)],
            &key,
        );
        let manifest = parse(&zip, &key).unwrap();
        assert_eq!((manifest.depot, manifest.id), (108600, 42));
        assert_eq!(manifest.files[0].path, "mods/Example");
        assert!(manifest.files[0].is_dir);
        assert_eq!(manifest.files[1].path, "mods/Example/mod.info");
        let offsets: Vec<_> = manifest.files[1].chunks.iter().map(|c| c.offset).collect();
        assert_eq!(offsets, [0, 2]);
    }

    #[test]
    fn refuses_paths_outside_the_install() {
        let key = [4u8; 32];
        for evil in [
            "..\\..\\evil",
            "/etc/passwd",
            "C:\\evil",
            "a\\\\b",
            "mods\\CON.txt",
            "mods\\trailing.",
            "mods\\bad\u{1}name",
        ] {
            assert!(
                parse(&manifest_zip(&[(evil, 0)], &key), &key).is_err(),
                "{evil}"
            );
        }
    }
}
