//! Steam's symmetric scheme: the first 16 bytes are the IV encrypted with AES-256-ECB, the rest is
//! AES-256-CBC with that IV and PKCS7 padding.

use aes::Aes256;
use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, KeyInit};

use crate::{Error, Result};

pub type Key = [u8; 32];

pub fn decrypt(key: &Key, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 32 || !data.len().is_multiple_of(16) {
        return Err(Error::Packet("encrypted data has a bad length"));
    }
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut iv = GenericArray::clone_from_slice(&data[..16]);
    cipher.decrypt_block(&mut iv);

    let mut out = Vec::with_capacity(data.len() - 16);
    let mut prev = iv;
    for block in data[16..].as_chunks::<16>().0 {
        let mut b = GenericArray::clone_from_slice(block);
        cipher.decrypt_block(&mut b);
        out.extend(b.iter().zip(prev.iter()).map(|(x, p)| x ^ p));
        prev = GenericArray::clone_from_slice(block);
    }

    let pad = *out.last().expect("at least one block") as usize;
    let valid =
        (1..=16).contains(&pad) && out[out.len() - pad..].iter().all(|&b| b as usize == pad);
    if !valid {
        return Err(Error::Packet("bad padding (wrong depot key?)"));
    }
    out.truncate(out.len() - pad);
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use aes::cipher::BlockEncrypt;

    use super::*;

    /// The inverse of [`decrypt`], only needed to build test data.
    pub fn encrypt(key: &Key, iv: [u8; 16], plain: &[u8]) -> Vec<u8> {
        let cipher = Aes256::new(GenericArray::from_slice(key));
        let pad = 16 - plain.len() % 16;
        let mut data = plain.to_vec();
        data.extend(std::iter::repeat_n(pad as u8, pad));

        let mut head = GenericArray::from(iv);
        cipher.encrypt_block(&mut head);
        let mut out = head.to_vec();
        let mut prev = GenericArray::from(iv);
        for block in data.as_chunks::<16>().0 {
            let mut b = GenericArray::clone_from_slice(block);
            b.iter_mut().zip(prev.iter()).for_each(|(x, p)| *x ^= p);
            cipher.encrypt_block(&mut b);
            out.extend_from_slice(&b);
            prev = b;
        }
        out
    }

    #[test]
    fn decrypts_steam_scheme() {
        let key = [7u8; 32];
        let plain = b"mods/Example/mod.info";
        let data = encrypt(&key, [3u8; 16], plain);
        assert_eq!(decrypt(&key, &data).unwrap(), plain);
        assert!(
            decrypt(&[8u8; 32], &data).is_err(),
            "wrong key must fail the padding check"
        );
        assert!(decrypt(&key, &data[..20]).is_err());
    }
}
