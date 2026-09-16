//! On-disk chunk format: compress, then encrypt, then write.
//!
//! The format is fixed now, deliberately, even though key *management* is not
//! built yet. Changing a stored format after there is stored data means
//! rewriting every chunk a user owns; changing how keys are derived does not.
//! So the expensive half is settled first.
//!
//! ```text
//! offset  size  field
//! 0       4     magic "QRB1"
//! 4       1     format version
//! 5       1     flags: bit 0 = zstd compressed
//! 6       2     reserved, must be zero
//! 8       24    XChaCha20-Poly1305 nonce
//! 32      ..    ciphertext, with a 16-byte authentication tag appended
//! ```
//!
//! The 32-byte header is passed as associated data, so it is authenticated
//! even though it is not encrypted. Tampering with the flags to make a reader
//! skip decompression will fail the tag check rather than produce garbage.

use crate::error::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

pub const MAGIC: &[u8; 4] = b"QRB1";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 32;
const NONCE_OFFSET: usize = 8;
const NONCE_LEN: usize = 24;

const FLAG_ZSTD: u8 = 0b0000_0001;

/// Compression level. 3 is zstd's default: most of the ratio for a small
/// fraction of the time of higher levels, which matters because chunking is
/// already disk-bound and we do not want to make it CPU-bound.
const ZSTD_LEVEL: i32 = 3;

/// Compression is kept only if it saves at least this fraction of the chunk.
/// Below that the CPU cost of decompressing on every read is not worth it, and
/// already-compressed formats like JPEG and MP4 land here naturally without
/// needing a list of file extensions to special-case.
const COMPRESSION_THRESHOLD: f64 = 0.95;

/// The symmetric key used for chunk payloads.
///
/// For now this is supplied by the caller. Deriving it from a master secret and
/// a recovery phrase is a later phase; that change will not affect the bytes on
/// disk, only where the key comes from.
#[derive(Clone)]
pub struct ChunkKey([u8; 32]);

impl ChunkKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Generate a fresh random key. Test and development use; real keys will
    /// come from the key hierarchy.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut k = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut k);
        Self(k)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new((&self.0).into())
    }
}

impl std::fmt::Debug for ChunkKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChunkKey(<redacted>)")
    }
}

/// Compress and encrypt a chunk for storage.
///
/// A random 192-bit nonce is used per call. At that width random nonces are
/// safe without tracking a counter, which matters because chunks are written
/// from several places and a shared counter would be a synchronisation point.
pub fn seal(key: &ChunkKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    let compressed = zstd::encode_all(plaintext, ZSTD_LEVEL)
        .map_err(|e| Error::io("<zstd>", e))?;

    let (body, flags) = if (compressed.len() as f64) < plaintext.len() as f64 * COMPRESSION_THRESHOLD
    {
        (compressed, FLAG_ZSTD)
    } else {
        (plaintext.to_vec(), 0)
    };

    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4] = VERSION;
    header[5] = flags;
    // bytes 6..8 stay zero (reserved)
    {
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut header[NONCE_OFFSET..NONCE_OFFSET + NONCE_LEN]);
    }

    let nonce = XNonce::from_slice(&header[NONCE_OFFSET..NONCE_OFFSET + NONCE_LEN]);
    let ciphertext = key
        .cipher()
        .encrypt(nonce, Payload { msg: &body, aad: &header })
        .map_err(|_| Error::Decrypt { hash: "<sealing>".into() })?;

    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt and decompress a stored chunk.
///
/// `hash` is used only to make errors identify the offending chunk.
pub fn open(key: &ChunkKey, stored: &[u8], hash: &str) -> Result<Vec<u8>> {
    if stored.len() < HEADER_LEN {
        return Err(Error::ChunkFormat {
            hash: hash.to_string(),
            detail: format!("truncated: {} bytes, need at least {HEADER_LEN}", stored.len()),
        });
    }
    let (header, ciphertext) = stored.split_at(HEADER_LEN);

    if &header[..4] != MAGIC {
        return Err(Error::ChunkFormat { hash: hash.to_string(), detail: "bad magic".into() });
    }
    if header[4] != VERSION {
        return Err(Error::ChunkFormat {
            hash: hash.to_string(),
            detail: format!("unsupported format version {}", header[4]),
        });
    }

    let nonce = XNonce::from_slice(&header[NONCE_OFFSET..NONCE_OFFSET + NONCE_LEN]);
    let body = key
        .cipher()
        .decrypt(nonce, Payload { msg: ciphertext, aad: header })
        .map_err(|_| Error::Decrypt { hash: hash.to_string() })?;

    if header[5] & FLAG_ZSTD != 0 {
        zstd::decode_all(body.as_slice()).map_err(|e| Error::io("<zstd>", e))
    } else {
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_compressible() {
        let key = ChunkKey::generate();
        let data = vec![b'a'; 100_000];
        let sealed = seal(&key, &data).unwrap();
        assert!(sealed.len() < data.len(), "repetitive data should compress");
        assert_eq!(open(&key, &sealed, "test").unwrap(), data);
    }

    #[test]
    fn round_trip_incompressible() {
        use rand::RngCore;
        let key = ChunkKey::generate();
        let mut data = vec![0u8; 100_000];
        rand::thread_rng().fill_bytes(&mut data);

        let sealed = seal(&key, &data).unwrap();
        assert_eq!(sealed[5] & FLAG_ZSTD, 0, "random data must not be stored compressed");
        assert_eq!(open(&key, &sealed, "test").unwrap(), data);
    }

    #[test]
    fn round_trip_empty() {
        let key = ChunkKey::generate();
        let sealed = seal(&key, b"").unwrap();
        assert_eq!(open(&key, &sealed, "test").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn wrong_key_is_rejected() {
        let sealed = seal(&ChunkKey::generate(), b"secret").unwrap();
        assert!(matches!(
            open(&ChunkKey::generate(), &sealed, "test"),
            Err(Error::Decrypt { .. })
        ));
    }

    #[test]
    fn tampering_with_ciphertext_is_detected() {
        let key = ChunkKey::generate();
        let mut sealed = seal(&key, b"hello world").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xFF;
        assert!(matches!(open(&key, &sealed, "test"), Err(Error::Decrypt { .. })));
    }

    #[test]
    fn tampering_with_the_header_is_detected() {
        // The header is not encrypted, so it must at least be authenticated:
        // flipping the compression flag has to fail rather than mis-decode.
        let key = ChunkKey::generate();
        let mut sealed = seal(&key, &vec![b'a'; 10_000]).unwrap();
        sealed[5] ^= FLAG_ZSTD;
        assert!(matches!(open(&key, &sealed, "test"), Err(Error::Decrypt { .. })));
    }

    #[test]
    fn nonces_differ_between_seals() {
        let key = ChunkKey::generate();
        let a = seal(&key, b"same input").unwrap();
        let b = seal(&key, b"same input").unwrap();
        assert_ne!(a, b, "nonce reuse would be catastrophic for this cipher");
    }

    #[test]
    fn truncated_input_is_rejected() {
        let key = ChunkKey::generate();
        let sealed = seal(&key, b"hello").unwrap();
        assert!(matches!(
            open(&key, &sealed[..10], "test"),
            Err(Error::ChunkFormat { .. })
        ));
    }
}
