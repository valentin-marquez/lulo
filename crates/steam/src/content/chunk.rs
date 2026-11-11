//! A depot chunk as served by the CDN: encrypted, compressed, then checked against the manifest.

use std::io::Read;

use sha1::{Digest, Sha1};

use super::crypto::{self, Key};
use super::manifest::Chunk;
use crate::{Error, Result};

/// Decrypts, decompresses and verifies one chunk.
pub fn open(key: &Key, raw: &[u8], chunk: &Chunk) -> Result<Vec<u8>> {
    if raw.len() != chunk.compressed as usize {
        return Err(Error::Packet("chunk size differs from the manifest"));
    }
    let plain = decompress(&crypto::decrypt(key, raw)?, chunk.original as usize)?;
    if plain.len() != chunk.original as usize
        || adler32(&plain) != chunk.crc
        || Sha1::digest(&plain).as_slice() != chunk.sha.as_slice()
    {
        return Err(Error::Packet("chunk failed verification"));
    }
    Ok(plain)
}

/// The manifest's `crc` is Adler-32 seeded with 0, not the standard 1.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (0u32, 0u32);
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("4 bytes"))
}

/// `size` is the expected output length; it bounds every decoder.
fn decompress(buf: &[u8], size: usize) -> Result<Vec<u8>> {
    // Grown as data arrives instead of trusting `size` up front.
    let mut out = Vec::new();
    if buf.starts_with(b"VSZa") {
        // "VSZa" | crc u32 | zstd frame | crc u32 | size u32 | u32 0 | "zsv"
        if buf.len() < 23 || !buf.ends_with(b"zsv") {
            return Err(Error::Packet("bad VSZa framing"));
        }
        let frame = &buf[8..buf.len() - 15];
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(frame)
            .map_err(|_| Error::Packet("bad zstd frame"))?;
        decoder
            .by_ref()
            .take(size as u64 + 1)
            .read_to_end(&mut out)?;
        if le_u32(&buf[buf.len() - 11..buf.len() - 7]) as usize != out.len() {
            return Err(Error::Packet("VSZa size mismatch"));
        }
    } else if buf.starts_with(b"VZa") {
        // "VZa" | u32 | 5 bytes LZMA props | data | crc u32 | size u32 | "zv"
        if buf.len() < 22 || !buf.ends_with(b"zv") {
            return Err(Error::Packet("bad VZa framing"));
        }
        // lzma-rs wants the classic .lzma header: props (5 bytes) + unpacked size (u64 LE).
        let mut stream = buf[7..12].to_vec();
        stream.extend_from_slice(&(size as u64).to_le_bytes());
        stream.extend_from_slice(&buf[12..buf.len() - 10]);
        lzma_rs::lzma_decompress(&mut stream.as_slice(), &mut out)
            .map_err(|_| Error::Packet("bad LZMA stream"))?;
    } else if buf.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(buf))
            .map_err(|_| Error::Packet("bad zip chunk"))?;
        let mut entry = zip
            .by_index(0)
            .map_err(|_| Error::Packet("empty zip chunk"))?;
        entry.by_ref().take(size as u64 + 1).read_to_end(&mut out)?;
    } else {
        return Err(Error::Packet("unknown chunk compression"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::crypto::tests::encrypt;

    #[test]
    fn adler32_uses_seed_zero() {
        // a = 97 + 98 + 99 = 294, b = 97 + 195 + 294 = 586
        assert_eq!(adler32(b"abc"), (586 << 16) | 294);
    }

    #[test]
    fn opens_an_lzma_chunk() {
        let plain = b"local function init() print('hello') end\n".repeat(20);
        let mut lzma = Vec::new();
        lzma_rs::lzma_compress(&mut plain.as_slice(), &mut lzma).unwrap();
        // .lzma = props (5) + size (8) + data; VZa keeps props and data.
        let mut vza = b"VZa\0\0\0\0".to_vec();
        vza.extend_from_slice(&lzma[..5]);
        vza.extend_from_slice(&lzma[13..]);
        vza.extend_from_slice(&0u32.to_le_bytes());
        vza.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        vza.extend_from_slice(b"zv");

        let key = [9u8; 32];
        let raw = encrypt(&key, [1u8; 16], &vza);
        let chunk = Chunk {
            sha: Sha1::digest(&plain).to_vec(),
            crc: adler32(&plain),
            offset: 0,
            original: plain.len() as u32,
            compressed: raw.len() as u32,
        };
        assert_eq!(open(&key, &raw, &chunk).unwrap(), plain);

        let tampered = Chunk {
            crc: chunk.crc ^ 1,
            ..chunk
        };
        assert!(open(&key, &raw, &tampered).is_err());
    }
}
