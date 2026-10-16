//! Packet Tracer's obfuscation, authenticated encryption and qCompress layer.

use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::{
    fmt, fs,
    io::{Read, Write},
    path::Path,
};

use crate::crypto::{eax_decrypt, eax_decrypt_unchecked, eax_encrypt};

const KEY: [u8; 16] = [0x89; 16];
const NONCE: [u8; 16] = [0x10; 16];

#[derive(Debug)]
pub enum PktError {
    TooShort,
    Authentication,
    Truncated,
    SizeOverflow,
    Zlib(std::io::Error),
    Io(std::io::Error),
}

impl fmt::Display for PktError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "file qua ngan de la .pkt"),
            Self::Authentication => write!(
                f,
                "EAX tag khong khop (sai key/IV hoac dinh dang file khac)"
            ),
            Self::Truncated => write!(f, "du lieu qCompress bi cat ngan"),
            Self::SizeOverflow => write!(f, "XML qua lon cho dinh dang qCompress"),
            Self::Zlib(error) => write!(f, "giai nen zlib that bai: {error}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for PktError {}

fn outer_decode(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    (0..n)
        .map(|i| data[n - 1 - i] ^ n.wrapping_sub(i.wrapping_mul(n)) as u8)
        .collect()
}

fn outer_encode(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut out = vec![0; n];
    for i in 0..n {
        out[n - 1 - i] = data[i] ^ n.wrapping_sub(i.wrapping_mul(n)) as u8;
    }
    out
}

fn inner(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    data.iter()
        .enumerate()
        .map(|(i, byte)| byte ^ n.wrapping_sub(i) as u8)
        .collect()
}

pub fn pkt_to_xml(data: &[u8]) -> Result<Vec<u8>, PktError> {
    pkt_to_xml_with_verify(data, true)
}

pub fn pkt_to_xml_with_verify(data: &[u8], verify: bool) -> Result<Vec<u8>, PktError> {
    // Four qCompress length bytes plus the 16-byte EAX tag is the true
    // structural minimum.  Real Packet Tracer documents are much larger,
    // while accepting this minimum makes xml_to_pkt round-trip tiny inputs.
    if data.len() < 20 {
        return Err(PktError::TooShort);
    }
    let outer = outer_decode(data);
    let split = outer.len() - 16;
    let tag: [u8; 16] = outer[split..].try_into().expect("tag length");
    let plaintext = if verify {
        eax_decrypt(KEY, NONCE, &outer[..split], tag).ok_or(PktError::Authentication)?
    } else {
        eax_decrypt_unchecked(KEY, NONCE, &outer[..split])
    };
    let compressed = inner(&plaintext);
    if compressed.len() < 4 {
        return Err(PktError::Truncated);
    }
    let declared = u32::from_be_bytes(compressed[..4].try_into().expect("size length")) as usize;
    let mut decoder = ZlibDecoder::new(&compressed[4..]);
    let mut xml = Vec::new();
    decoder.read_to_end(&mut xml).map_err(PktError::Zlib)?;
    if declared != xml.len() {
        eprintln!(
            "[!] canh bao: kich thuoc khai bao {declared} != thuc te {}",
            xml.len()
        );
    }
    Ok(xml)
}

pub fn xml_to_pkt(xml: &[u8]) -> Result<Vec<u8>, PktError> {
    let size = u32::try_from(xml.len()).map_err(|_| PktError::SizeOverflow)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(xml).map_err(PktError::Io)?;
    let payload = encoder.finish().map_err(PktError::Io)?;
    let mut qcompress = Vec::with_capacity(payload.len() + 4);
    qcompress.extend_from_slice(&size.to_be_bytes());
    qcompress.extend_from_slice(&payload);
    let (ciphertext, tag) = eax_encrypt(KEY, NONCE, &inner(&qcompress));
    let mut encrypted = ciphertext;
    encrypted.extend_from_slice(&tag);
    Ok(outer_encode(&encrypted))
}

pub fn looks_like_xml(data: &[u8]) -> bool {
    let head = &data[..data.len().min(200)];
    let trimmed = head
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map_or(&[][..], |start| &head[start..]);
    trimmed.starts_with(b"<?xml") || trimmed.starts_with(b"<PACKETTRACER")
}

pub fn load_xml(path: impl AsRef<Path>) -> Result<Vec<u8>, PktError> {
    let data = fs::read(path).map_err(PktError::Io)?;
    if looks_like_xml(&data) {
        Ok(data)
    } else {
        pkt_to_xml(&data)
    }
}
