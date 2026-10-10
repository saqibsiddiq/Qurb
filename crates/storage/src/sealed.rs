//! A guest's files, as the computer keeping them sees them (decision 0060).
//!
//! A guest -- another person's device visiting a computer -- may have that
//! computer keep its Private Vault, and the computer must not be able to read
//! it. So what the computer is shown is sealed under a key only the guest
//! holds, derived from the guest's own key for that computer:
//!
//! - a **sealed name** in place of each path: the real path, encrypted,
//!   written as `~` and base64url. Sealed the same way every time, so a file
//!   keeps its sealed name as it changes and a new version replaces the old
//!   one on the computer, and the sealed name of a file deleted here can be
//!   worked out to delete it there. The computer stores a path it cannot
//!   read; it can tell only that two versions are of one file.
//! - **sealed chunks** in place of the bytes: each chunk encrypted with a
//!   nonce derived from the chunk itself, so the same chunk always seals to
//!   the same bytes, behind a sealed **header** holding the file's real size,
//!   content hash, time and the length of each chunk. The computer is shown
//!   the file those make, and keeps and checks it exactly as it would any
//!   file. It never sees a hash of the plain text, so it cannot test whether a
//!   guest has a file it knows.
//!
//! The sealed file is read back as one stream: the computer cuts what it
//! keeps into chunks of its own, as it does any file, so nothing can rely on
//! where its chunks begin. Hence the layout -- the header's length in four
//! bytes, the sealed header, then each sealed chunk, whose lengths the header
//! gives.
//!
//! Deterministic sealing tells the computer when two of one guest's chunks
//! are the same, which is what lets it keep a file it already has once. It
//! tells it nothing across guests, or across computers, since each has its
//! own key.

use crate::format::ChunkKey;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use qurb_sync::DeviceId;

const NONCE_LEN: usize = 24;
/// What marks a path as a sealed name.
const MARK: char = '~';
/// The longest path sealed. A sealed name is the path on the wire, which
/// allows 4 KiB; this leaves room for what sealing adds.
pub const MAX_SEALED_PATH: usize = 2000;

/// The key a guest seals its vault under for one computer.
#[derive(Clone)]
pub struct FolderKey([u8; 32]);

impl FolderKey {
    /// Derived from this device's chunk key -- itself from the person's own
    /// key -- and the computer's device id: one computer's key gives nothing
    /// for another.
    pub fn for_host(chunk_key: &ChunkKey, host: &DeviceId) -> Self {
        Self(chunk_key.derive(b"qurb/sealed-folder/v1", host.as_bytes()))
    }

    /// A folder key as its bytes, which a guest's phone sends the computer
    /// keeping its folder to open it there, and which that computer holds
    /// only while it is open (decision 0060).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    fn sub(&self, label: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_keyed(&self.0);
        hasher.update(label);
        *hasher.finalize().as_bytes()
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new((&self.sub(b"cipher")).into())
    }
}

impl std::fmt::Debug for FolderKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FolderKey(<redacted>)")
    }
}

/// What a sealed file's header holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    /// The file's real path, in the guest's own vault.
    pub path: String,
    /// Its real size.
    pub size: u64,
    /// The hash of its plain text, which the guest checks a fetched file
    /// against.
    pub content: blake3::Hash,
    /// When it changed, in unix seconds.
    pub modified_at: i64,
    /// The length of each chunk of plain text, in order: how the sealed
    /// chunks after the header are told apart.
    pub chunks: Vec<u32>,
}

/// What every sealed chunk adds to its plain text: the nonce and the tag.
pub const SEAL_OVERHEAD: usize = NONCE_LEN + 16;

/// Seal a path as one the computer can store and not read, the same way
/// every time. `None` for a path too long to seal.
pub fn seal_name(key: &FolderKey, path: &str) -> Option<String> {
    if path.len() > MAX_SEALED_PATH {
        return None;
    }
    Some(format!("{MARK}{}", base64url::encode(&seal_with(key, b"name-nonce", path.as_bytes()))))
}

/// Open a sealed name. `None` for a path that is not one, or not sealed with
/// this key, or tampered with.
pub fn open_name(key: &FolderKey, sealed: &str) -> Option<String> {
    let bytes = base64url::decode(sealed.strip_prefix(MARK)?)?;
    String::from_utf8(open_with(key, &bytes)?).ok()
}

/// The start of a sealed file: the sealed header's length, then the sealed
/// header -- the real path, size, content hash, time and chunk lengths.
pub fn seal_header(key: &FolderKey, meta: &Meta) -> Vec<u8> {
    let mut plain = Vec::with_capacity(meta.path.len() + 60 + 4 * meta.chunks.len());
    plain.extend_from_slice(b"QRBH");
    plain.extend_from_slice(&(meta.path.len() as u32).to_le_bytes());
    plain.extend_from_slice(meta.path.as_bytes());
    plain.extend_from_slice(&meta.size.to_le_bytes());
    plain.extend_from_slice(meta.content.as_bytes());
    plain.extend_from_slice(&meta.modified_at.to_le_bytes());
    plain.extend_from_slice(&(meta.chunks.len() as u32).to_le_bytes());
    for len in &meta.chunks {
        plain.extend_from_slice(&len.to_le_bytes());
    }
    let sealed = seal_chunk(key, &plain);
    let mut out = (sealed.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&sealed);
    out
}

/// Open the start of a sealed file made by [`seal_header`]. `None` for
/// anything that is not one, under this key.
pub fn open_header(key: &FolderKey, start: &[u8]) -> Option<Meta> {
    let len = u32::from_le_bytes(start.get(..4)?.try_into().ok()?) as usize;
    let plain = open_chunk(key, start.get(4..4 + len)?)?;
    let mut r = plain.strip_prefix(b"QRBH")?;
    let take = |r: &mut &[u8], n: usize| -> Option<Vec<u8>> {
        if r.len() < n {
            return None;
        }
        let (head, tail) = r.split_at(n);
        *r = tail;
        Some(head.to_vec())
    };
    let len = u32::from_le_bytes(take(&mut r, 4)?.try_into().ok()?) as usize;
    let path = String::from_utf8(take(&mut r, len)?).ok()?;
    let size = u64::from_le_bytes(take(&mut r, 8)?.try_into().ok()?);
    let content = blake3::Hash::from(<[u8; 32]>::try_from(take(&mut r, 32)?).ok()?);
    let modified_at = i64::from_le_bytes(take(&mut r, 8)?.try_into().ok()?);
    let count = u32::from_le_bytes(take(&mut r, 4)?.try_into().ok()?) as usize;
    if count > r.len() / 4 {
        return None;
    }
    let mut chunks = Vec::with_capacity(count);
    for _ in 0..count {
        chunks.push(u32::from_le_bytes(take(&mut r, 4)?.try_into().ok()?));
    }
    r.is_empty().then_some(Meta { path, size, content, modified_at, chunks })
}

/// Reads a sealed file back from pieces cut anywhere: the header first, then
/// each chunk opened as soon as all of it has arrived.
pub struct Unsealer {
    key: FolderKey,
    pending: Vec<u8>,
    meta: Option<Meta>,
    next: usize,
}

impl Unsealer {
    pub fn new(key: FolderKey) -> Self {
        Self { key, pending: Vec::new(), meta: None, next: 0 }
    }

    /// The header, once it has arrived and opened.
    pub fn meta(&self) -> Option<&Meta> {
        self.meta.as_ref()
    }

    /// Take a piece, and give back whatever plain text it completes, in
    /// order. An error for anything that does not open under this key.
    pub fn push(&mut self, piece: &[u8]) -> Result<Vec<Vec<u8>>, &'static str> {
        self.pending.extend_from_slice(piece);
        let mut out = Vec::new();
        if self.meta.is_none() {
            let Some(len) = self.pending.get(..4).map(|b| u32::from_le_bytes(b.try_into().expect("4"))) else {
                return Ok(out);
            };
            let end = 4 + len as usize;
            if self.pending.len() < end {
                return Ok(out);
            }
            let meta = open_header(&self.key, &self.pending[..end]).ok_or("its header does not open with this key")?;
            self.pending.drain(..end);
            self.meta = Some(meta);
        }
        let meta = self.meta.as_ref().expect("set above");
        while let Some(len) = meta.chunks.get(self.next) {
            let sealed = *len as usize + SEAL_OVERHEAD;
            if self.pending.len() < sealed {
                break;
            }
            let plain = open_chunk(&self.key, &self.pending[..sealed]).ok_or("a chunk does not open with this key")?;
            self.pending.drain(..sealed);
            self.next += 1;
            out.push(plain);
        }
        Ok(out)
    }

    /// Whether every chunk the header promised has arrived, and nothing more.
    pub fn finished(&self) -> bool {
        self.meta.as_ref().is_some_and(|m| self.next == m.chunks.len()) && self.pending.is_empty()
    }
}

/// Whether a path is a sealed name -- something a guest sealed -- whatever
/// the key.
pub fn is_sealed_name(path: &str) -> bool {
    path.starts_with(MARK) && !path.contains('/')
}

/// Seal one chunk so the same plain text always seals to the same bytes:
/// the nonce is derived from the chunk under the folder key.
pub fn seal_chunk(key: &FolderKey, plain: &[u8]) -> Vec<u8> {
    seal_with(key, b"nonce", plain)
}

/// Open a sealed chunk. `None` for one not sealed with this key, or altered.
pub fn open_chunk(key: &FolderKey, sealed: &[u8]) -> Option<Vec<u8>> {
    open_with(key, sealed)
}

/// Encrypt with a nonce derived from the plain text, under a label of its
/// own: the same input always gives the same output, and nothing else does.
fn seal_with(key: &FolderKey, label: &[u8], plain: &[u8]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new_keyed(&key.sub(label));
    hasher.update(plain);
    let nonce: [u8; NONCE_LEN] = hasher.finalize().as_bytes()[..NONCE_LEN].try_into().expect("24 of 32");
    let sealed = key
        .cipher()
        .encrypt(XNonce::from_slice(&nonce), plain)
        .expect("encrypting in memory cannot fail");
    let mut out = nonce.to_vec();
    out.extend_from_slice(&sealed);
    out
}

fn open_with(key: &FolderKey, sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < NONCE_LEN {
        return None;
    }
    let (nonce, body) = sealed.split_at(NONCE_LEN);
    key.cipher().decrypt(XNonce::from_slice(nonce), body).ok()
}

/// RFC 4648 base64url, without padding: the alphabet a path can carry.
mod base64url {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 4 / 3 + 3);
        for group in bytes.chunks(3) {
            let n = group.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
            for i in 0..=group.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    pub fn decode(text: &str) -> Option<Vec<u8>> {
        let values: Option<Vec<u32>> = text
            .bytes()
            .map(|c| ALPHABET.iter().position(|a| *a == c).map(|p| p as u32))
            .collect();
        let values = values?;
        if values.len() % 4 == 1 {
            return None;
        }
        let mut out = Vec::with_capacity(values.len() * 3 / 4);
        for group in values.chunks(4) {
            let n = group.iter().enumerate().fold(0u32, |n, (i, v)| n | v << (18 - 6 * i));
            for i in 0..group.len() - 1 {
                out.push((n >> (16 - 8 * i)) as u8);
            }
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        /// RFC 4648's vectors, unpadded, in the URL-safe alphabet.
        #[test]
        fn matches_the_standard() {
            for (plain, coded) in
                [("", ""), ("f", "Zg"), ("fo", "Zm8"), ("foo", "Zm9v"), ("foob", "Zm9vYg"), ("fooba", "Zm9vYmE"), ("foobar", "Zm9vYmFy")]
            {
                assert_eq!(super::encode(plain.as_bytes()), coded);
                assert_eq!(super::decode(coded).unwrap(), plain.as_bytes());
            }
            assert_eq!(super::encode(&[0xfb, 0xff]), "-_8");
            assert_eq!(super::decode("-_8").unwrap(), vec![0xfb, 0xff]);
            assert!(super::decode("A").is_none());
            assert!(super::decode("a+b").is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(host: u8) -> FolderKey {
        FolderKey::for_host(&ChunkKey::from_bytes([7; 32]), &DeviceId::from_bytes([host; 32]))
    }

    fn meta() -> Meta {
        Meta {
            path: "Photos/2026/beach.jpg".into(),
            size: 4_700_000,
            content: blake3::hash(b"the beach"),
            modified_at: 1_791_000_000,
            chunks: vec![524_288, 1_000_000],
        }
    }

    #[test]
    fn a_name_seals_and_opens_the_same_way_each_time() {
        let sealed = seal_name(&key(1), "Photos/2026/beach.jpg").unwrap();
        assert!(is_sealed_name(&sealed), "{sealed}");
        assert!(qurb_sync::is_safe_path(&sealed));
        assert!(!sealed.contains("beach") && !sealed.contains("Photos"), "{sealed}");
        assert_eq!(open_name(&key(1), &sealed).unwrap(), "Photos/2026/beach.jpg");
        assert_eq!(seal_name(&key(1), "Photos/2026/beach.jpg").unwrap(), sealed);
        assert_ne!(seal_name(&key(1), "Photos/2026/beach2.jpg").unwrap(), sealed);
    }

    /// Sealed for one computer, it is nothing to another, or to anyone who
    /// changed a byte of it.
    #[test]
    fn a_name_opens_only_with_its_key() {
        let sealed = seal_name(&key(1), "notes.txt").unwrap();
        assert!(open_name(&key(2), &sealed).is_none());
        assert_ne!(seal_name(&key(2), "notes.txt").unwrap(), sealed);
        let mut altered: Vec<char> = sealed.chars().collect();
        let last = altered.len() - 1;
        altered[last] = if altered[last] == 'A' { 'B' } else { 'A' };
        assert!(open_name(&key(1), &altered.into_iter().collect::<String>()).is_none());
        assert!(open_name(&key(1), "Photos/beach.jpg").is_none());
    }

    #[test]
    fn a_long_path_is_refused() {
        assert!(seal_name(&key(1), &"x".repeat(MAX_SEALED_PATH + 1)).is_none());
    }

    #[test]
    fn a_header_seals_and_opens() {
        let header = seal_header(&key(1), &meta());
        assert_eq!(open_header(&key(1), &header).unwrap(), meta());
        assert!(open_header(&key(2), &header).is_none());
        assert!(open_header(&key(1), &seal_chunk(&key(1), b"not a header")).is_none());
    }

    /// Read back from pieces cut anywhere -- as the computer keeping it cuts
    /// it -- a sealed file opens to what was sealed.
    #[test]
    fn a_sealed_file_reads_back_from_pieces_cut_anywhere() {
        let chunks: Vec<Vec<u8>> = vec![vec![1u8; 3000], vec![2u8; 70], vec![3u8; 5000]];
        let meta = Meta {
            path: "notes.txt".into(),
            size: 8070,
            content: blake3::hash(&chunks.concat()),
            modified_at: 7,
            chunks: chunks.iter().map(|c| c.len() as u32).collect(),
        };
        let mut sealed = seal_header(&key(1), &meta);
        for chunk in &chunks {
            sealed.extend(seal_chunk(&key(1), chunk));
        }
        for cut in [1, 7, 100, 4096, sealed.len()] {
            let mut reader = Unsealer::new(key(1));
            let mut plain = Vec::new();
            for piece in sealed.chunks(cut) {
                for opened in reader.push(piece).unwrap() {
                    plain.extend(opened);
                }
            }
            assert!(reader.finished(), "cut {cut}");
            assert_eq!(plain, chunks.concat(), "cut {cut}");
            assert_eq!(reader.meta().unwrap(), &meta);
        }
        assert!(Unsealer::new(key(2)).push(&sealed).is_err(), "opened with another key");
    }

    /// The same chunk seals the same way each time, so the computer can keep
    /// it once; a different computer's key seals it differently.
    #[test]
    fn a_chunk_seals_the_same_way_each_time() {
        let plain = b"one chunk of somebody's file";
        let once = seal_chunk(&key(1), plain);
        assert_eq!(once, seal_chunk(&key(1), plain));
        assert_ne!(once, seal_chunk(&key(2), plain));
        assert_ne!(once, seal_chunk(&key(1), b"another chunk entirely......"));
        assert!(!once.windows(plain.len()).any(|w| w == plain), "the plain text shows through");
        assert_eq!(open_chunk(&key(1), &once).unwrap(), plain);
        assert!(open_chunk(&key(2), &once).is_none());
    }
}
