//! Content delivery: depot keys, CDN hosts, manifests, and installing a manifest's files.

pub mod chunk;
pub mod crypto;
pub mod manifest;

use std::path::Path;
use std::time::{Duration, Instant};

use sha1::{Digest, Sha1};

use crate::cm::packet::emsg;
use crate::proto::{
    CContentServerDirectoryGetManifestRequestCodeRequest,
    CContentServerDirectoryGetManifestRequestCodeResponse,
    CContentServerDirectoryGetServersForSteamPipeRequest,
    CContentServerDirectoryGetServersForSteamPipeResponse, CMsgClientGetDepotDecryptionKey,
    CMsgClientGetDepotDecryptionKeyResponse,
};
use crate::{Client, Error, Result, cm};
pub use crypto::Key;
pub use manifest::Manifest;

const MANIFEST_CAP: usize = 256 << 20;

/// The AES key of a depot. Anonymous sessions get keys for free content only.
pub async fn depot_key(client: &mut Client, app: u32, depot: u32) -> Result<Key> {
    let reply: CMsgClientGetDepotDecryptionKeyResponse = client
        .request(
            emsg::CLIENT_GET_DEPOT_DECRYPTION_KEY,
            emsg::CLIENT_GET_DEPOT_DECRYPTION_KEY_RESPONSE,
            &CMsgClientGetDepotDecryptionKey {
                depot_id: Some(depot),
                app_id: Some(app),
            },
        )
        .await?;
    cm::check(reply.eresult, &format!("depot key for {depot}"))?;
    reply
        .depot_encryption_key
        .as_deref()
        .and_then(|k| k.try_into().ok())
        .ok_or(Error::Packet("depot key is not 32 bytes"))
}

/// CDN hosts that serve HTTPS, nearest first as Steam orders them for our cell.
pub async fn cdn_hosts(client: &mut Client) -> Result<Vec<String>> {
    let request = CContentServerDirectoryGetServersForSteamPipeRequest {
        cell_id: Some(client.cell_id),
    };
    let reply: CContentServerDirectoryGetServersForSteamPipeResponse = client
        .call("ContentServerDirectory.GetServersForSteamPipe", &request)
        .await?;
    let hosts: Vec<String> = reply
        .servers
        .into_iter()
        .filter(|s| matches!(s.https_support(), "mandatory" | "optional"))
        .filter_map(|s| s.host)
        .collect();
    if hosts.is_empty() {
        return Err(Error::NoServers);
    }
    Ok(hosts)
}

/// Downloads and parses manifest `id` of `depot`. `branch` is the app branch it belongs to
/// (`public` for Workshop items and the default game branch).
pub async fn manifest(
    client: &mut Client,
    http: &reqwest::Client,
    host: &str,
    (app, depot, id): (u32, u32, u64),
    branch: &str,
    key: &Key,
) -> Result<Manifest> {
    let request = CContentServerDirectoryGetManifestRequestCodeRequest {
        app_id: Some(app),
        depot_id: Some(depot),
        manifest_id: Some(id),
        app_branch: Some(branch.to_owned()),
    };
    // The code expires within minutes, so fetch it right before the download.
    let code: CContentServerDirectoryGetManifestRequestCodeResponse = client
        .call("ContentServerDirectory.GetManifestRequestCode", &request)
        .await?;
    let url = format!(
        "https://{host}/depot/{depot}/manifest/{id}/5/{}",
        code.manifest_request_code()
    );
    let zip = crate::read_capped(http.get(url).send().await?, MANIFEST_CAP, "manifest").await?;
    manifest::parse(&zip, key)
}

/// Chunks downloaded at once. They finish in any order and are written at their offset, so
/// one slow chunk doesn't hold back the others.
const PARALLEL: usize = 32;

/// Tries per chunk, each on the next CDN host, before the install fails.
const TRIES: usize = 3;

/// Where an install's time went. Network and decode are summed over chunks in flight at once,
/// so they can add up to more than the wall time; compare them with each other.
#[derive(Debug, Default, Clone, Copy)]
pub struct Timings {
    pub network: Duration,
    pub decode: Duration,
    pub write: Duration,
}

/// Writes every file of `manifest` under `dir`, verifying each chunk. Files already on disk
/// with the right size and SHA-1 are kept, so an interrupted install resumes; files that match
/// under `seed` (an older install) are copied from there instead of downloaded. `progress`
/// receives the bytes done so far, kept and copied files included.
// ponytail: a file that changed is downloaded whole; reusing its unchanged chunks (updates
// between builds) is #24.
pub async fn install(
    http: &reqwest::Client,
    hosts: &[String],
    (depot, key): (u32, &Key),
    manifest: &Manifest,
    dir: &Path,
    seed: Option<&Path>,
    mut progress: impl FnMut(u64),
) -> Result<Timings> {
    use futures_util::{StreamExt, stream};

    if hosts.is_empty() {
        return Err(Error::NoServers);
    }
    // Folders and empty files first; everything else is written as its chunks arrive.
    for file in &manifest.files {
        let path = target(dir, &file.path)?;
        if file.is_dir {
            tokio::fs::create_dir_all(&path).await?;
        } else if file.chunks.is_empty() {
            if file.size != 0 {
                return Err(Error::Packet("file has a size but no chunks"));
            }
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::File::create(&path).await?;
            executable(&path).await?;
        }
    }

    let mut left = vec![0; manifest.files.len()];
    let mut done = 0;
    for (ix, file) in manifest.files.iter().enumerate() {
        if file.is_dir || file.chunks.is_empty() {
            continue;
        }
        let path = target(dir, &file.path)?;
        if verified(path.clone(), file).await {
            done += file.size;
            continue;
        }
        if let Some(seed) = seed {
            let from = target(seed, &file.path)?;
            if verified(from.clone(), file).await {
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                tokio::fs::copy(&from, &path).await?;
                executable(&path).await?;
                done += file.size;
                continue;
            }
        }
        // Each chunk is checked against its SHA-1; if they also tile the file exactly, the
        // file is right once all of them are written, whatever order they arrive in.
        if !tiles(file) {
            return Err(Error::Packet("chunks don't tile the file"));
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::File::create(&path)
            .await?
            .set_len(file.size)
            .await?;
        left[ix] = file.chunks.len();
    }
    if done > 0 {
        progress(done);
    }

    // Owned jobs and a plain async fn keep the stream `Send`, so installs can be spawned.
    let mut jobs = Vec::new();
    for (ix, file) in manifest.files.iter().enumerate() {
        if left[ix] > 0 {
            let path = std::sync::Arc::new(target(dir, &file.path)?);
            jobs.extend(file.chunks.iter().map(|c| (ix, path.clone(), c.clone())));
        }
    }
    let mut chunks = stream::iter(jobs)
        .map(|(ix, path, c)| fetch(http, hosts, depot, *key, ix, path, c))
        .buffer_unordered(PARALLEL);

    let mut timings = Timings::default();
    while let Some(next) = chunks.next().await {
        let (ix, path, written, spent) = next?;
        timings.network += spent.network;
        timings.decode += spent.decode;
        timings.write += spent.write;
        done += written;
        progress(done);
        left[ix] -= 1;
        if left[ix] == 0 {
            executable(&path).await?;
        }
    }
    Ok(timings)
}

/// Whether the chunks, sorted by offset, cover the file end to end with no gaps or overlaps.
fn tiles(file: &manifest::File) -> bool {
    let mut end = 0;
    for chunk in &file.chunks {
        if chunk.offset != end {
            return false;
        }
        end += u64::from(chunk.original);
    }
    end == file.size
}

/// Whether `path` already holds `file`: same size and SHA-1.
async fn verified(path: std::path::PathBuf, file: &manifest::File) -> bool {
    let (size, sha) = (file.size, file.sha.clone());
    tokio::task::spawn_blocking(move || {
        if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(size) {
            return false;
        }
        let mut hash = Sha1::new();
        std::fs::File::open(&path)
            .and_then(|mut f| std::io::copy(&mut f, &mut hash))
            .is_ok()
            && hash.finalize().as_slice() == sha.as_slice()
    })
    .await
    .unwrap_or(false)
}

async fn fetch(
    http: &reqwest::Client,
    hosts: &[String],
    depot: u32,
    key: Key,
    ix: usize,
    path: std::sync::Arc<std::path::PathBuf>,
    chunk: manifest::Chunk,
) -> Result<(usize, std::sync::Arc<std::path::PathBuf>, u64, Timings)> {
    let mut spent = Timings::default();
    let mut tries = 0;
    let plain = loop {
        let host = &hosts[(ix + tries) % hosts.len()];
        tries += 1;
        match download(http, host, depot, key, &chunk, &mut spent).await {
            Ok(plain) => break plain,
            // A dropped connection or a corrupt chunk is usually one bad host or a hiccup.
            Err(err) if tries < TRIES && !matches!(err, Error::Io(_)) => continue,
            Err(err) => return Err(err),
        }
    };
    let started = Instant::now();
    let written = plain.len() as u64;
    let file = path.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::{Seek, SeekFrom, Write};
        let mut out = std::fs::OpenOptions::new().write(true).open(&*file)?;
        out.seek(SeekFrom::Start(chunk.offset))?;
        out.write_all(&plain)
    })
    .await
    .map_err(|_| Error::Packet("chunk writer stopped"))??;
    spent.write = started.elapsed();
    Ok((ix, path, written, spent))
}

/// One chunk from one host, decrypted and verified.
async fn download(
    http: &reqwest::Client,
    host: &str,
    depot: u32,
    key: Key,
    chunk: &manifest::Chunk,
    spent: &mut Timings,
) -> Result<Vec<u8>> {
    let started = Instant::now();
    let url = format!("https://{host}/depot/{depot}/chunk/{}", chunk.id());
    let raw = crate::read_capped(
        http.get(url).send().await?,
        chunk.compressed as usize,
        "chunk",
    )
    .await;
    spent.network += started.elapsed();
    let raw = raw?;
    // Decrypt and LZMA take most of an install's CPU; on the blocking pool they use every core
    // instead of the one thread polling the stream.
    let chunk = chunk.clone();
    let (plain, decode) = tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        chunk::open(&key, &raw, &chunk).map(|plain| (plain, started.elapsed()))
    })
    .await
    .map_err(|_| Error::Packet("chunk decoder stopped"))??;
    spent.decode += decode;
    Ok(plain)
}

/// `dir/path`, refusing anything that would land outside `dir`. `manifest::parse` already
/// rejects unsafe paths; this keeps install safe on its own.
fn target(dir: &Path, path: &str) -> Result<std::path::PathBuf> {
    let joined = dir.join(path);
    if !joined.starts_with(dir) {
        return Err(Error::Packet("path escapes the install folder"));
    }
    Ok(joined)
}

/// Manifests carry no executable bit; SteamCMD creates every file 0755 on Unix, and the game's
/// launch scripts need it.
async fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).await?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(body: &[u8]) -> manifest::File {
        manifest::File {
            path: "f".into(),
            size: body.len() as u64,
            is_dir: false,
            sha: Sha1::digest(body).to_vec(),
            chunks: Vec::new(),
        }
    }

    #[tokio::test]
    async fn keeps_only_files_that_match() {
        let dir = std::env::temp_dir().join(format!("steam-verified-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f");
        std::fs::write(&path, b"abc").unwrap();

        assert!(verified(path.clone(), &file(b"abc")).await);
        assert!(
            !verified(path.clone(), &file(b"abd")).await,
            "same size, other bytes"
        );
        assert!(!verified(path.clone(), &file(b"ab")).await, "other size");
        assert!(!verified(dir.join("missing"), &file(b"abc")).await);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn chunks_must_tile_the_file() {
        let chunk = |offset, original| manifest::Chunk {
            sha: Vec::new(),
            crc: 0,
            offset,
            original,
            compressed: 0,
        };
        let with = |size, chunks| manifest::File {
            size,
            chunks,
            ..file(b"")
        };
        assert!(tiles(&with(10, vec![chunk(0, 4), chunk(4, 6)])));
        assert!(!tiles(&with(10, vec![chunk(0, 4), chunk(5, 5)])), "gap");
        assert!(!tiles(&with(10, vec![chunk(0, 6), chunk(4, 6)])), "overlap");
        assert!(!tiles(&with(12, vec![chunk(0, 4), chunk(4, 6)])), "short");
    }
}
