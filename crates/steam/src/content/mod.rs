//! Content delivery: depot keys, CDN hosts, manifests, and installing a manifest's files.

pub mod chunk;
pub mod crypto;
pub mod manifest;

use std::path::Path;

use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

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

/// A CDN host that serves HTTPS, nearest first as Steam orders them for our cell.
pub async fn cdn_host(client: &mut Client) -> Result<String> {
    let request = CContentServerDirectoryGetServersForSteamPipeRequest {
        cell_id: Some(client.cell_id),
    };
    let reply: CContentServerDirectoryGetServersForSteamPipeResponse = client
        .call("ContentServerDirectory.GetServersForSteamPipe", &request)
        .await?;
    reply
        .servers
        .into_iter()
        .find(|s| matches!(s.https_support(), "mandatory" | "optional"))
        .and_then(|s| s.host)
        .ok_or(Error::NoServers)
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

/// Chunks downloaded at once. Order is kept, so files are still written front to back.
const PARALLEL: usize = 16;

/// Writes every file of `manifest` under `dir`, verifying each chunk and each whole file.
/// Files already on disk with the right size and SHA-1 are kept, so an interrupted install
/// resumes. `progress` receives the bytes done so far, kept files included.
// ponytail: a file that changed is downloaded whole; reusing its unchanged chunks (updates
// between builds) is #24.
pub async fn install(
    http: &reqwest::Client,
    host: &str,
    depot: u32,
    key: &Key,
    manifest: &Manifest,
    dir: &Path,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    use futures_util::{StreamExt, stream};

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

    let mut kept = vec![false; manifest.files.len()];
    let mut done = 0;
    for (ix, file) in manifest.files.iter().enumerate() {
        if !file.is_dir && !file.chunks.is_empty() && verified(target(dir, &file.path)?, file).await
        {
            kept[ix] = true;
            done += file.size;
        }
    }
    if done > 0 {
        progress(done);
    }

    // Owned jobs and a plain async fn keep the stream `Send`, so installs can be spawned.
    let jobs: Vec<(usize, manifest::Chunk)> = manifest
        .files
        .iter()
        .enumerate()
        .filter(|(ix, f)| !f.is_dir && !kept[*ix])
        .flat_map(|(ix, f)| f.chunks.iter().cloned().map(move |c| (ix, c)))
        .collect();
    let mut chunks = stream::iter(jobs)
        .map(|(ix, c)| fetch(http, host, depot, key, ix, c))
        .buffered(PARALLEL);

    let mut open: Option<Open> = None;
    while let Some(next) = chunks.next().await {
        let (ix, c, plain) = next?;
        if open.as_ref().is_none_or(|o| o.ix != ix) {
            if let Some(previous) = open.take() {
                previous.finish(manifest).await?;
            }
            let path = target(dir, &manifest.files[ix].path)?;
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            open = Some(Open {
                ix,
                out: tokio::fs::File::create(&path).await?,
                path,
                hash: Sha1::new(),
                written: 0,
            });
        }
        let file = open.as_mut().expect("opened above");
        // Chunks are sorted by offset; they must tile the file with no gaps or overlaps.
        if c.offset != file.written || file.written + plain.len() as u64 > manifest.files[ix].size {
            return Err(Error::Packet("chunks don't tile the file"));
        }
        file.out.write_all(&plain).await?;
        file.hash.update(&plain);
        file.written += plain.len() as u64;
        done += plain.len() as u64;
        progress(done);
    }
    if let Some(last) = open {
        last.finish(manifest).await?;
    }
    Ok(())
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
    host: &str,
    depot: u32,
    key: &Key,
    ix: usize,
    chunk: manifest::Chunk,
) -> Result<(usize, manifest::Chunk, Vec<u8>)> {
    let url = format!("https://{host}/depot/{depot}/chunk/{}", chunk.id());
    let raw = crate::read_capped(
        http.get(url).send().await?,
        chunk.compressed as usize,
        "chunk",
    )
    .await?;
    let plain = chunk::open(key, &raw, &chunk)?;
    Ok((ix, chunk, plain))
}

/// The file being written while its chunks stream in.
struct Open {
    ix: usize,
    path: std::path::PathBuf,
    out: tokio::fs::File,
    hash: Sha1,
    written: u64,
}

impl Open {
    async fn finish(mut self, manifest: &Manifest) -> Result<()> {
        let file = &manifest.files[self.ix];
        self.out.flush().await?;
        if self.written != file.size || self.hash.finalize().as_slice() != file.sha.as_slice() {
            return Err(Error::Packet("file failed verification"));
        }
        executable(&self.path).await
    }
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
}
