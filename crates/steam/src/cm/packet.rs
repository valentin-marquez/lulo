//! Steam packet framing: one WebSocket binary frame carries one packet.
//!
//! `u32 LE (emsg | PROTO_MASK) | u32 LE header length | CMsgProtoBufHeader | body`.
//! `Multi` packets bundle several packets, optionally gzipped, as `[u32 len][packet]...`.

use prost::Message;

use crate::proto::{CMsgMulti, CMsgProtoBufHeader};
use crate::{Error, Result};

const PROTO_MASK: u32 = 0x8000_0000;
/// CM bundles are small (KB to a few MB). The budget is shared by the whole expansion, so
/// nested bundles can't multiply it.
const MULTI_CAP: u64 = 32 << 20;
/// Steam nests at most one Multi inside another; deeper is refused.
const MULTI_DEPTH: usize = 2;

/// Message ids we use (`enums_clientserver.proto`).
pub mod emsg {
    pub const MULTI: u32 = 1;
    pub const DEST_JOB_FAILED: u32 = 113;
    pub const SERVICE_METHOD_RESPONSE: u32 = 147;
    pub const SERVICE_METHOD_CALL: u32 = 151;
    pub const CLIENT_HEART_BEAT: u32 = 703;
    pub const CLIENT_LOGON_RESPONSE: u32 = 751;
    pub const CLIENT_GET_DEPOT_DECRYPTION_KEY: u32 = 5438;
    pub const CLIENT_GET_DEPOT_DECRYPTION_KEY_RESPONSE: u32 = 5439;
    pub const CLIENT_LOGON: u32 = 5514;
    pub const CLIENT_PICS_PRODUCT_INFO_REQUEST: u32 = 8903;
    pub const CLIENT_PICS_PRODUCT_INFO_RESPONSE: u32 = 8904;
    pub const CLIENT_PICS_ACCESS_TOKEN_REQUEST: u32 = 8905;
    pub const CLIENT_PICS_ACCESS_TOKEN_RESPONSE: u32 = 8906;
    pub const SERVICE_METHOD_CALL_NON_AUTHED: u32 = 9804;
    pub const CLIENT_HELLO: u32 = 9805;
}

#[derive(Debug)]
pub struct Packet {
    pub emsg: u32,
    pub header: CMsgProtoBufHeader,
    pub body: Vec<u8>,
}

pub fn encode(emsg: u32, header: &CMsgProtoBufHeader, body: &impl Message) -> Vec<u8> {
    let header = header.encode_to_vec();
    let mut out = Vec::with_capacity(8 + header.len() + body.encoded_len());
    out.extend_from_slice(&(emsg | PROTO_MASK).to_le_bytes());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    body.encode(&mut out).expect("Vec has room");
    out
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let raw = bytes.get(at..at + 4).ok_or(Error::Packet("truncated"))?;
    Ok(u32::from_le_bytes(raw.try_into().expect("4 bytes")))
}

pub fn decode(bytes: &[u8]) -> Result<Packet> {
    let raw = u32_at(bytes, 0)?;
    if raw & PROTO_MASK == 0 {
        return Ok(legacy(raw, bytes));
    }
    let len = u32_at(bytes, 4)? as usize;
    let header = bytes
        .get(8..8 + len)
        .ok_or(Error::Packet("header past end"))?;
    Ok(Packet {
        emsg: raw & !PROTO_MASK,
        header: CMsgProtoBufHeader::decode(header)?,
        body: bytes[8 + len..].to_vec(),
    })
}

/// A packet in the old non-protobuf framing. We only read the target job of `DestJobFailed`, so a
/// request whose job died fails instead of waiting forever; other legacy packets are kept empty.
fn legacy(emsg: u32, bytes: &[u8]) -> Packet {
    // ExtendedClientMsgHdr: emsg u32 | header size u8 | version u16 | target job u64 | ...
    let target = bytes
        .get(7..15)
        .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")));
    Packet {
        emsg,
        header: CMsgProtoBufHeader {
            jobid_target: if emsg == emsg::DEST_JOB_FAILED {
                target
            } else {
                None
            },
            ..Default::default()
        },
        body: Vec::new(),
    }
}

/// Flattens `Multi` packets (recursively) into the packets they carry.
pub fn expand(packet: Packet) -> Result<Vec<Packet>> {
    let mut budget = MULTI_CAP;
    let mut out = Vec::new();
    expand_into(packet, 0, &mut budget, &mut out)?;
    Ok(out)
}

fn expand_into(
    packet: Packet,
    depth: usize,
    budget: &mut u64,
    out: &mut Vec<Packet>,
) -> Result<()> {
    if packet.emsg != emsg::MULTI {
        out.push(packet);
        return Ok(());
    }
    if depth >= MULTI_DEPTH {
        return Err(Error::Packet("Multi nested too deep"));
    }
    let multi = CMsgMulti::decode(packet.body.as_slice())?;
    let body = multi.message_body.unwrap_or_default();
    let data = if multi.size_unzipped.unwrap_or(0) > 0 {
        crate::gunzip(&body, *budget, "Multi packet")?
    } else {
        body
    };
    *budget = budget
        .checked_sub(data.len() as u64)
        .ok_or(Error::TooLarge("Multi packet"))?;

    let mut at = 0;
    while at < data.len() {
        let len = u32_at(&data, at)? as usize;
        let inner = data
            .get(at + 4..at + 4 + len)
            .ok_or(Error::Packet("multi entry past end"))?;
        expand_into(decode(inner)?, depth + 1, budget, out)?;
        at += 4 + len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::{Compression, write::GzEncoder};

    use super::*;
    use crate::proto::CMsgClientHello;

    fn hello(job: u64) -> Vec<u8> {
        let header = CMsgProtoBufHeader {
            jobid_source: Some(job),
            ..Default::default()
        };
        let body = CMsgClientHello {
            protocol_version: Some(65580),
        };
        encode(emsg::CLIENT_HELLO, &header, &body)
    }

    #[test]
    fn round_trips() {
        let packet = decode(&hello(7)).unwrap();
        assert_eq!(packet.emsg, emsg::CLIENT_HELLO);
        assert_eq!(packet.header.jobid_source, Some(7));
        let body = CMsgClientHello::decode(packet.body.as_slice()).unwrap();
        assert_eq!(body.protocol_version, Some(65580));
    }

    #[test]
    fn reads_dest_job_failed_and_rejects_truncated() {
        // Legacy header: emsg 113, size 36, version 2, target job 7, ...
        let mut legacy = 113u32.to_le_bytes().to_vec();
        legacy.push(36);
        legacy.extend_from_slice(&2u16.to_le_bytes());
        legacy.extend_from_slice(&7u64.to_le_bytes());
        let packet = decode(&legacy).unwrap();
        assert_eq!(
            (packet.emsg, packet.header.jobid_target),
            (emsg::DEST_JOB_FAILED, Some(7))
        );

        assert!(decode(&[1, 0]).is_err());
        let mut bad = hello(1);
        bad[4] = 0xff; // header length past the end
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn expands_gzipped_multi() {
        let mut inner = Vec::new();
        for job in [1u64, 2] {
            let packet = hello(job);
            inner.extend_from_slice(&(packet.len() as u32).to_le_bytes());
            inner.extend_from_slice(&packet);
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&inner).unwrap();
        let multi = CMsgMulti {
            size_unzipped: Some(inner.len() as u32),
            message_body: Some(gz.finish().unwrap()),
        };
        let packet = decode(&encode(emsg::MULTI, &CMsgProtoBufHeader::default(), &multi)).unwrap();

        let jobs: Vec<_> = expand(packet)
            .unwrap()
            .iter()
            .map(|p| p.header.jobid_source)
            .collect();
        assert_eq!(jobs, [Some(1), Some(2)]);
    }

    #[test]
    fn refuses_deeply_nested_multi() {
        let mut packet = hello(1);
        for _ in 0..=MULTI_DEPTH {
            let mut body = (packet.len() as u32).to_le_bytes().to_vec();
            body.extend_from_slice(&packet);
            let multi = CMsgMulti {
                size_unzipped: None,
                message_body: Some(body),
            };
            packet = encode(emsg::MULTI, &CMsgProtoBufHeader::default(), &multi);
        }
        assert!(expand(decode(&packet).unwrap()).is_err());
    }
}
