//! Native Steam client for lulo: CM connection over WebSocket, PICS app info, content delivery.
//!
//! Protocol notes and the reference implementation live outside the repo (see CLAUDE.md).

pub mod auth;
pub mod cm;
pub mod content;
pub mod directory;
pub mod keyvalues;
pub mod pics;
pub mod workshop;

pub mod proto;

pub use cm::Client;
/// Re-exported so callers can name the HTTP client type `steam::http()` returns.
pub use reqwest;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("websocket: {0}")]
    Socket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("protobuf: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed packet: {0}")]
    Packet(&'static str),
    #[error("malformed KeyValues: {0}")]
    KeyValues(#[from] keyvalues::ParseError),
    #[error("{context} failed with EResult {code}")]
    Steam { code: i32, context: String },
    #[error("connection closed by Steam")]
    Closed,
    #[error("no reachable CM server")]
    NoServers,
    #[error("{0} is larger than allowed")]
    TooLarge(&'static str),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// HTTP client for Steam's web endpoints and CDN. Installs the TLS provider on first use.
pub fn http() -> reqwest::Client {
    tls();
    reqwest::Client::new()
}

/// rustls needs a process-wide crypto provider; we ship `ring` so builds need no C toolchain
/// beyond what Rust already requires on every OS.
fn tls() {
    // Err only means it was already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Reads a response body, refusing more than `cap` bytes (by header and while streaming).
pub(crate) async fn read_capped(
    reply: reqwest::Response,
    cap: usize,
    what: &'static str,
) -> Result<Vec<u8>> {
    let mut reply = reply.error_for_status()?;
    if reply.content_length().is_some_and(|n| n > cap as u64) {
        return Err(Error::TooLarge(what));
    }
    let mut out = Vec::new();
    while let Some(chunk) = reply.chunk().await? {
        if out.len() + chunk.len() > cap {
            return Err(Error::TooLarge(what));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Gunzips `data`, refusing to produce more than `cap` bytes (decompression bombs).
pub(crate) fn gunzip(data: &[u8], cap: u64, what: &'static str) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data)
        .take(cap + 1)
        .read_to_end(&mut out)?;
    if out.len() as u64 > cap {
        return Err(Error::TooLarge(what));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    #[test]
    fn gunzip_enforces_cap() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&[0u8; 4096]).unwrap();
        let data = gz.finish().unwrap();
        assert_eq!(super::gunzip(&data, 4096, "test").unwrap().len(), 4096);
        assert!(matches!(
            super::gunzip(&data, 4095, "test"),
            Err(super::Error::TooLarge(_))
        ));
    }
}
