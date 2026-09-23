//! The wire format.
//!
//! Hand-rolled and explicit, for two reasons. Every field is length-prefixed
//! and bounded, so a malformed or hostile message fails a check rather than
//! allocating whatever it asks for. And the format is small enough to read in
//! one sitting, which matters for something two versions of the software must
//! agree on for years.
//!
//! ```text
//! request   [u8 tag][payload]
//! response  [u8 status][payload]
//! ```
//!
//! Framing is left to QUIC: each exchange is one bidirectional stream, so the
//! stream's own end marks the end of the message and no length header is
//! needed around it. Streams are cheap and independent, which is exactly why
//! the transport was chosen.

use crate::error::{Error, Result};
use qurb_sync::{Content, DeviceId, FileVersion, VersionVector};

/// Upper bound on any single decoded message.
///
/// A chunk is at most 2 MiB and a tree is bounded by the file count, but a
/// peer that claims otherwise must be refused rather than believed: without a
/// cap, one four-byte length field is an out-of-memory attack.
pub const MAX_MESSAGE: usize = 64 << 20;

/// The largest path a peer may claim. Generous against any real filesystem and
/// still far too small to be useful as an allocation attack.
const MAX_PATH: usize = 4 << 10;

/// A device name is for humans to read. Anything longer is not a name.
const MAX_NAME: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Everything this device knows about, tombstones included.
    Tree,
    /// The chunk hashes making up a file with this content hash.
    ///
    /// The step that makes transfer incremental: the requester compares the
    /// list against what it already holds and asks only for the difference.
    Manifest { content: [u8; 32] },
    /// One chunk's plaintext.
    Chunk { hash: [u8; 32] },
    /// Tell me when your tree changes.
    ///
    /// The reply comes when the peer's state has moved past `since`, or after a
    /// while with the current value if nothing happens.
    ///
    /// This does not break the rule that a peer can ask and never tell. The
    /// device that wants to know is the one asking; the answer simply arrives
    /// later than usual. A peer still cannot make anything happen here.
    Changes { since: u64 },

    /// Tell a peer this device now holds that content.
    ///
    /// Sent after a transfer completes, by the device that received it. The
    /// only message in the protocol that asks for nothing: the sender has
    /// something the receiver wants to know, rather than the other way round.
    ///
    /// It is what lets a device tell "waiting to be delivered" from "delivered"
    /// — the difference between a phone that can say your photo reached the
    /// desktop and one that can only say it tried. It is also what makes it
    /// safe for the *sender* to later drop its own copy under a storage cap.
    ///
    /// Which device sent it is taken from the connection's certificate, not
    /// from the message, so a peer cannot claim delivery on another's behalf.
    Got { content: [u8; 32] },

    /// Ask to be trusted, presenting the token from an out-of-band invite.
    ///
    /// The device id and name are claims; the fingerprint that ends up trusted
    /// is taken from the connection, not from here.
    Pair {
        token: [u8; 16],
        device_id: [u8; 32],
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Tree(Vec<FileVersion>),
    Manifest(Vec<[u8; 32]>),
    Chunk(Vec<u8>),
    /// The peer does not have what was asked for. Not an error: content moves
    /// and a peer may legitimately have dropped it.
    NotFound,
    /// Pairing accepted, with the accepting device's own identity.
    Paired { device_id: [u8; 32], name: String },

    /// Acknowledgement of [`Request::Got`]. Carries nothing.
    Noted,

    /// Where the peer's state has got to.
    ///
    /// Returned both when something changed and when the wait timed out, since
    /// the asking device wants the current value either way.
    Changed { generation: u64 },
}

const TAG_TREE: u8 = 1;
const TAG_MANIFEST: u8 = 2;
const TAG_CHUNK: u8 = 3;
const TAG_PAIR: u8 = 4;
const TAG_CHANGES: u8 = 5;
const TAG_GOT: u8 = 6;

const STATUS_TREE: u8 = 1;
const STATUS_MANIFEST: u8 = 2;
const STATUS_CHUNK: u8 = 3;
const STATUS_NOT_FOUND: u8 = 4;
const STATUS_PAIRED: u8 = 5;
const STATUS_CHANGED: u8 = 6;
const STATUS_NOTED: u8 = 7;

impl Request {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(33);
        match self {
            Request::Tree => out.push(TAG_TREE),
            Request::Manifest { content } => {
                out.push(TAG_MANIFEST);
                out.extend_from_slice(content);
            }
            Request::Chunk { hash } => {
                out.push(TAG_CHUNK);
                out.extend_from_slice(hash);
            }
            Request::Changes { since } => {
                out.push(TAG_CHANGES);
                out.extend_from_slice(&since.to_le_bytes());
            }
            Request::Got { content } => {
                out.push(TAG_GOT);
                out.extend_from_slice(content);
            }
            Request::Pair { token, device_id, name } => {
                out.push(TAG_PAIR);
                out.extend_from_slice(token);
                out.extend_from_slice(device_id);
                put_name(&mut out, name);
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let request = match r.u8()? {
            TAG_TREE => Request::Tree,
            TAG_MANIFEST => Request::Manifest { content: r.hash()? },
            TAG_CHUNK => Request::Chunk { hash: r.hash()? },
            TAG_CHANGES => Request::Changes { since: r.u64()? },
            TAG_GOT => Request::Got { content: r.hash()? },
            TAG_PAIR => {
                let mut token = [0u8; 16];
                token.copy_from_slice(r.take(16)?);
                Request::Pair { token, device_id: r.hash()?, name: r.name()? }
            }
            tag => return Err(Error::Protocol { detail: format!("unknown request tag {tag}") }),
        };
        r.finished()?;
        Ok(request)
    }
}

impl Response {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Response::NotFound => out.push(STATUS_NOT_FOUND),
            Response::Noted => out.push(STATUS_NOTED),
            Response::Changed { generation } => {
                out.push(STATUS_CHANGED);
                out.extend_from_slice(&generation.to_le_bytes());
            }
            Response::Paired { device_id, name } => {
                out.push(STATUS_PAIRED);
                out.extend_from_slice(device_id);
                put_name(&mut out, name);
            }
            Response::Tree(versions) => {
                out.push(STATUS_TREE);
                put_u32(&mut out, versions.len());
                for v in versions {
                    encode_version(&mut out, v);
                }
            }
            Response::Manifest(hashes) => {
                out.push(STATUS_MANIFEST);
                put_u32(&mut out, hashes.len());
                for h in hashes {
                    out.extend_from_slice(h);
                }
            }
            Response::Chunk(bytes) => {
                out.push(STATUS_CHUNK);
                put_u32(&mut out, bytes.len());
                out.extend_from_slice(bytes);
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let response = match r.u8()? {
            STATUS_NOT_FOUND => Response::NotFound,
            STATUS_NOTED => Response::Noted,
            STATUS_CHANGED => Response::Changed { generation: r.u64()? },
            STATUS_PAIRED => {
                Response::Paired { device_id: r.hash()?, name: r.name()? }
            }
            STATUS_TREE => {
                let count = r.count()?;
                let mut versions = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    versions.push(decode_version(&mut r)?);
                }
                Response::Tree(versions)
            }
            STATUS_MANIFEST => {
                let count = r.count()?;
                let mut hashes = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    hashes.push(r.hash()?);
                }
                Response::Manifest(hashes)
            }
            STATUS_CHUNK => {
                let len = r.count()?;
                Response::Chunk(r.take(len)?.to_vec())
            }
            status => {
                return Err(Error::Protocol { detail: format!("unknown response status {status}") })
            }
        };
        r.finished()?;
        Ok(response)
    }
}

fn encode_version(out: &mut Vec<u8>, v: &FileVersion) {
    let path = v.path.as_bytes();
    put_u32(out, path.len());
    out.extend_from_slice(path);

    match &v.content {
        Content::File { hash, size } => {
            out.push(0);
            out.extend_from_slice(hash);
            out.extend_from_slice(&size.to_le_bytes());
        }
        Content::Deleted => out.push(1),
    }

    let vector = v.vector.encode();
    put_u32(out, vector.len());
    out.extend_from_slice(&vector);

    out.extend_from_slice(v.modified_by.as_bytes());
    out.extend_from_slice(&v.modified_at.to_le_bytes());

    // Whether this entry belongs in the receiver's private vault. Sent
    // explicitly because the receiver cannot tell from the path, and gets one
    // chance to file it correctly: a private version adopted as shared content
    // would be advertised onward to every other device.
    out.push(v.private as u8);
}

fn decode_version(r: &mut Reader<'_>) -> Result<FileVersion> {
    let len = r.count()?;
    if len > MAX_PATH {
        return Err(Error::Protocol { detail: format!("path of {len} bytes is implausible") });
    }
    let path = std::str::from_utf8(r.take(len)?)
        .map_err(|_| Error::Protocol { detail: "path is not utf-8".into() })?
        .to_string();

    let content = match r.u8()? {
        0 => Content::File { hash: r.hash()?, size: r.u64()? },
        1 => Content::Deleted,
        other => {
            return Err(Error::Protocol { detail: format!("unknown content kind {other}") })
        }
    };

    let vector_len = r.count()?;
    let vector = VersionVector::decode(r.take(vector_len)?)
        .map_err(|e| Error::Protocol { detail: format!("bad version vector: {e}") })?;

    Ok(FileVersion {
        path,
        content,
        vector,
        modified_by: DeviceId::from_bytes(r.hash()?),
        modified_at: r.u64()? as i64,
        private: r.u8()? != 0,
    })
}

/// A device name, length-prefixed and bounded.
fn put_name(out: &mut Vec<u8>, name: &str) {
    let bytes = name.as_bytes();
    let capped = &bytes[..bytes.len().min(MAX_NAME)];
    put_u32(out, capped.len());
    out.extend_from_slice(capped);
}

fn put_u32(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

/// A cursor that refuses to read past the end, so every malformed message
/// produces an error instead of a panic.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or_else(|| Error::Protocol {
            detail: "length overflows".into(),
        })?;
        if end > self.bytes.len() {
            return Err(Error::Protocol {
                detail: format!("message ends early: wanted {n} bytes at offset {}", self.at),
            });
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(b))
    }

    /// A length field, checked against the message cap before it is trusted.
    fn count(&mut self) -> Result<usize> {
        let mut b = [0u8; 4];
        b.copy_from_slice(self.take(4)?);
        let n = u32::from_le_bytes(b) as usize;
        if n > MAX_MESSAGE {
            return Err(Error::Protocol { detail: format!("declared length {n} exceeds the cap") });
        }
        Ok(n)
    }

    /// A length-prefixed, bounded, valid-UTF-8 name.
    fn name(&mut self) -> Result<String> {
        let len = self.count()?;
        if len > MAX_NAME {
            return Err(Error::Protocol { detail: format!("name of {len} bytes is not a name") });
        }
        std::str::from_utf8(self.take(len)?)
            .map(|s| s.to_string())
            .map_err(|_| Error::Protocol { detail: "name is not utf-8".into() })
    }

    fn hash(&mut self) -> Result<[u8; 32]> {
        let mut h = [0u8; 32];
        h.copy_from_slice(self.take(32)?);
        Ok(h)
    }

    /// Reject trailing bytes rather than ignoring them: a message that decodes
    /// but has content left over means the two sides disagree about the format.
    fn finished(&self) -> Result<()> {
        if self.at != self.bytes.len() {
            return Err(Error::Protocol {
                detail: format!("{} trailing bytes", self.bytes.len() - self.at),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(path: &str) -> FileVersion {
        let mut vector = VersionVector::new();
        vector.set(DeviceId::from_bytes([7; 32]), 42);
        FileVersion::file(path, [3; 32], 1234, vector, DeviceId::from_bytes([9; 32]), 1_757_462_400)
    }

    #[test]
    fn requests_round_trip() {
        for r in [
            Request::Tree,
            Request::Manifest { content: [1; 32] },
            Request::Chunk { hash: [2; 32] },
        ] {
            assert_eq!(Request::decode(&r.encode()).unwrap(), r);
        }
    }

    #[test]
    fn responses_round_trip() {
        let responses = [
            Response::NotFound,
            Response::Tree(vec![]),
            Response::Tree(vec![version("a.txt"), version("dir/b bin.tar.gz")]),
            Response::Manifest(vec![[1; 32], [2; 32]]),
            Response::Chunk(vec![0xAB; 5000]),
            Response::Chunk(vec![]),
        ];
        for r in responses {
            assert_eq!(Response::decode(&r.encode()).unwrap(), r);
        }
    }

    #[test]
    fn tombstones_survive_the_wire() {
        // A deletion that failed to cross would resurrect the file.
        let tomb = FileVersion::tombstone(
            "gone.txt",
            VersionVector::new(),
            DeviceId::from_bytes([5; 32]),
            99,
        );
        let encoded = Response::Tree(vec![tomb.clone()]).encode();
        match Response::decode(&encoded).unwrap() {
            Response::Tree(v) => assert_eq!(v, vec![tomb]),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn version_vectors_survive_the_wire() {
        // Losing a vector would not corrupt content, but it would make every
        // version look concurrent and turn ordinary edits into conflicts.
        let original = version("a.txt");
        let encoded = Response::Tree(vec![original.clone()]).encode();
        match Response::decode(&encoded).unwrap() {
            Response::Tree(v) => assert_eq!(v[0].vector, original.vector),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn non_ascii_paths_survive() {
        let v = version("фото/日本語 файл.txt");
        let encoded = Response::Tree(vec![v.clone()]).encode();
        match Response::decode(&encoded).unwrap() {
            Response::Tree(got) => assert_eq!(got[0].path, v.path),
            other => panic!("got {other:?}"),
        }
    }

    // -- hostile input -------------------------------------------------------

    #[test]
    fn truncation_is_an_error_not_a_panic() {
        let encoded = Response::Tree(vec![version("a.txt")]).encode();
        for cut in 0..encoded.len() {
            assert!(Response::decode(&encoded[..cut]).is_err(), "accepted a {cut}-byte prefix");
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut encoded = Request::Tree.encode();
        encoded.push(0);
        assert!(Request::decode(&encoded).is_err());
    }

    #[test]
    fn an_absurd_length_is_refused_rather_than_allocated() {
        // Without the cap this is a four-byte out-of-memory attack.
        let mut hostile = vec![STATUS_CHUNK];
        hostile.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(Response::decode(&hostile).is_err());
    }

    #[test]
    fn an_absurd_path_length_is_refused() {
        let mut hostile = vec![STATUS_TREE];
        hostile.extend_from_slice(&1u32.to_le_bytes());
        hostile.extend_from_slice(&(MAX_PATH as u32 + 1).to_le_bytes());
        assert!(Response::decode(&hostile).is_err());
    }

    #[test]
    fn unknown_tags_are_refused() {
        assert!(Request::decode(&[99]).is_err());
        assert!(Response::decode(&[99]).is_err());
    }

    #[test]
    fn empty_input_is_an_error() {
        assert!(Request::decode(&[]).is_err());
        assert!(Response::decode(&[]).is_err());
    }

    #[test]
    fn invalid_utf8_in_a_path_is_refused() {
        let mut hostile = vec![STATUS_TREE];
        hostile.extend_from_slice(&1u32.to_le_bytes());
        hostile.extend_from_slice(&2u32.to_le_bytes());
        hostile.extend_from_slice(&[0xFF, 0xFE]);
        assert!(Response::decode(&hostile).is_err());
    }
}
