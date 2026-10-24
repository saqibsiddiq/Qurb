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
use qurb_storage::db::{Choice, FolderSummary};

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
    ///
    /// `kind` is `phone`, `computer` or `replica`, and `key_check` a one-way
    /// derivation of the key: two devices with different keys do not pair
    /// (decision 0053).
    Pair {
        token: [u8; 16],
        device_id: [u8; 32],
        name: String,
        kind: String,
        key_check: [u8; 32],
    },

    /// Ask for the key, as a device with none, presenting the invite's token
    /// (decision 0052). Answered with [`Response::Key`] once the person at the
    /// other device approves (decision 0053), once per invite; the device then
    /// sets itself up with it and pairs with [`Request::Pair`] on the same
    /// connection. The name and kind are for that approval.
    Join { token: [u8; 16], name: String, kind: String },

    /// What kind of device this is. Asked of a peer paired before devices
    /// said so when pairing.
    About,

    /// Ask to visit as a guest: another person's device, with its own key,
    /// presenting a guest invite's token (decision 0060). Answered with
    /// [`Response::Welcome`] once the person at the computer approves; no key
    /// is asked for or given.
    ///
    /// `person` is who the device's person is to this computer, derived from
    /// their key for it: the computer keeps a guest's folder under it, so a
    /// phone set up again with the same key finds the folder it had.
    Visit { token: [u8; 16], device_id: [u8; 32], name: String, kind: String, person: [u8; 32] },

    /// Whether this computer asks to open the asking guest's folder here
    /// (decision 0060). Answered with [`Response::Asks`].
    Asks,

    /// The guest approved, behind its screen lock: the folder's key, for the
    /// ask with this nonce. Held in memory only, while the folder is open.
    Unlock { nonce: [u8; 16], key: [u8; 32] },

    /// What folders this device has that could be backed up (decision 0063).
    /// Asked by a computer that keeps its vault, and answered to that computer
    /// only, with [`Response::Folders`].
    Folders,

    /// Which of the asking device's folders this computer was told to back
    /// up (decision 0063). Asked by a phone of the computer keeping its vault,
    /// answered with [`Response::Chosen`]. The phone asks; nothing is pushed
    /// at it.
    Chosen,

    /// The same as [`Request::Tree`], answered in frames rather than one
    /// message (decision 0065): each a length, then a count and that many
    /// versions, ending with an empty frame. One message is capped at
    /// [`MAX_MESSAGE`], and a million files' list is larger than that, so a
    /// library that size could not sync at all.
    TreeStream,

    /// Six digits tried with a device showing a code (decision 0066): this
    /// side's half of SPAKE2. Answered with [`Response::Short`], or
    /// [`Response::Withdrawn`] once the digits have been tried too often.
    Short { spake: [u8; SPAKE_LEN] },

    /// This side's confirmation that it derived the same key. Answered with
    /// the long code, [`Response::Sealed`] under that key, or *not found*.
    ShortConfirm { confirm: [u8; 32] },

    /// What a computer keeps of the asking phone's, at `dir` of its folder
    /// there (decision 0067, step 4): answered with [`Response::Kept`], to a
    /// phone of the computer's own person only, about itself and its earlier
    /// selves.
    Kept { dir: String },

    /// The chunks making up a file, each with its size: answered with
    /// [`Response::Layout`] to whoever may have its manifest, under the same
    /// rules. What reading part of a file needs -- a player seeking in a
    /// video kept on a computer -- since a chunk's place in the file is the
    /// sum of the sizes before it.
    Layout { content: [u8; 32] },

    /// The fingerprint of the file list [`Request::TreeStream`] would send
    /// now ([`tree_digest`]), so that a device holding that list already
    /// from an earlier sync need not fetch it again. A phone's list on a
    /// computer keeping its WhatsApp backup is tens of thousands of files,
    /// 9.8 MB a pass on the S23 with nothing changed: on a slow hotspot the
    /// pass ran out of time before the list arrived, every time (2026-10-19).
    TreeDigest,

    /// What changed in the file list since the list counter stood at
    /// `since` (decision 0072): answered by [`Response::TreeDelta`], or by
    /// [`Response::NotFound`] when the changes are too many to be worth
    /// sending apart from the list.
    TreeSince { since: u64 },

    /// Who the device showing a code is, asked by a device about to join or
    /// visit with it, presenting the code's token: answered with
    /// [`Response::Who`] before anybody approves anything, so the joining
    /// device can say where to approve it -- *Approve on saqib* -- rather than
    /// "your other device". Asked on a connection of its own: a device from
    /// before this does not know it, answers *not found* and drops that
    /// connection, and the join goes on as before on another.
    Who { token: [u8; 16] },

    /// One folder of this computer's shared folder, at `dir` (`""` for the
    /// top), live, to a phone of its own person or to a guest of it
    /// (decision 0073, item 6): answered in [`Response::Kept`]'s shape, each
    /// file's `owner` this computer and its `path` its place in the shared
    /// area, which is what fetching it names. Folders shared without the
    /// asker are left out. Never to another computer, which syncs the shared
    /// area instead.
    Shared { dir: String },

    /// Changes to this computer's shared folder, made by the computer on its
    /// own folder when one of its own person's phones or a guest asks
    /// (decision 0073, item 6) -- the exception to serving read-only, and
    /// only for these. Each is answered [`Response::Noted`] when made,
    /// [`Response::Refused`] with a reason a person can read, or
    /// [`Response::NotFound`] by a computer that takes no changes from the
    /// asker. A folder, made empty:
    SharedMkdir { path: String },
    /// A file or folder renamed or moved.
    SharedMove { from: String, to: String },
    /// A file or folder deleted, into the computer's Recently deleted.
    SharedDelete { path: String },
    /// A piece of a file being added, at `offset`, in order: `id` is the
    /// asker's own name for the upload, and the computer keeps the pieces
    /// apart from the folder until [`Request::UploadDone`].
    Upload { id: [u8; 16], offset: u64, bytes: Vec<u8> },
    /// The pieces of upload `id` are all sent: put the file at `path` once
    /// its bytes hash to `content`. Answered [`Response::Placed`], with where
    /// -- numbered when the name was taken.
    UploadDone { id: [u8; 16], path: String, content: [u8; 32] },
}

/// One file of a phone's folder on a computer, as listed to the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptEntry {
    /// Its name, as the folder shows it.
    pub name: String,
    /// Where it is kept, in the vault of `owner`.
    pub path: String,
    pub owner: [u8; 32],
    pub content: [u8; 32],
    pub size: u64,
    /// Unix seconds.
    pub modified_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Tree(Vec<FileVersion>),
    Manifest(Vec<[u8; 32]>),
    Chunk(Vec<u8>),
    /// The peer does not have what was asked for. Not an error: content moves
    /// and a peer may legitimately have dropped it.
    NotFound,
    /// Pairing accepted, with the accepting device's own identity, kind and
    /// key check.
    Paired { device_id: [u8; 32], name: String, kind: String, key_check: [u8; 32] },

    /// The device's kind, answering [`Request::About`].
    About { kind: String },

    /// Pairing refused: the two devices hold different keys.
    Mismatch,

    /// Pairing refused by the person at the device showing the code.
    Declined,

    /// The master key, for a device that asked with [`Request::Join`] and the
    /// invite's token. Only ever sent over a pairing connection, to a device
    /// that proved it saw the code.
    Key { key: [u8; 32] },

    /// Acknowledgement of [`Request::Got`]. Carries nothing.
    Noted,

    /// A guest welcomed, answering [`Request::Visit`]: the computer's own
    /// identity and kind, and the secret the two devices meet under at the
    /// rendezvous service (decision 0060).
    Welcome { device_id: [u8; 32], name: String, kind: String, meeting: [u8; 32] },

    /// The ask to open the guest's folder here, by its nonce, or none.
    Asks { opening: Option<[u8; 16]> },

    /// Where the peer's state has got to.
    ///
    /// Returned both when something changed and when the wait timed out, since
    /// the asking device wants the current value either way.
    Changed { generation: u64 },

    /// This device's folders, two levels deep, each with what is beneath it,
    /// and the choices made on it of which to back up, with when, answering
    /// [`Request::Folders`].
    Folders { folders: Vec<FolderSummary>, choices: Vec<Choice> },

    /// The choices made for the asking device of its folders to back up,
    /// with when, answering [`Request::Chosen`]. The later choice wins,
    /// wherever it was made (item 10).
    Chosen(Vec<Choice>),

    /// The device showing a code's half of SPAKE2, and its confirmation
    /// (decision 0066).
    Short { spake: [u8; SPAKE_LEN], confirm: [u8; 32] },

    /// The long code, sealed under the key six digits agreed.
    Sealed(Vec<u8>),

    /// The digits were tried too often, and are no longer taken for this
    /// code. The QR code and the long code still work.
    Withdrawn,

    /// A place in a phone's folder on a computer, answering [`Request::Kept`].
    Kept { folders: Vec<String>, files: Vec<KeptEntry> },

    /// A file's chunks, each with its size in bytes, answering
    /// [`Request::Layout`].
    Layout(Vec<([u8; 32], u32)>),

    /// The file list's fingerprint, answering [`Request::TreeDigest`].
    Digest([u8; 32]),

    /// Answering [`Request::TreeSince`]: where the list counter stands now,
    /// the whole list's fingerprint as it is now ([`tree_digest`]), and the
    /// entries of it that changed. The asker puts them in place of its own
    /// and checks the result against the fingerprint.
    TreeDelta { now: u64, digest: [u8; 32], changed: Vec<FileVersion> },

    /// The name and kind of the device showing a code, answering
    /// [`Request::Who`]. Claims, as a joining device's own name is: the
    /// number both screens show is what is checked.
    Who { name: String, kind: String },

    /// A change asked of the shared folder, not made, and why, in words for
    /// the person who asked (decision 0073).
    Refused { why: String },

    /// An added file, put in the shared folder at `path`.
    Placed { path: String },
}

const TAG_TREE: u8 = 1;
const TAG_MANIFEST: u8 = 2;
const TAG_CHUNK: u8 = 3;
const TAG_PAIR: u8 = 4;
const TAG_CHANGES: u8 = 5;
const TAG_GOT: u8 = 6;
const TAG_JOIN: u8 = 7;
const TAG_ABOUT: u8 = 8;
const TAG_VISIT: u8 = 9;
const TAG_ASKS: u8 = 10;
const TAG_UNLOCK: u8 = 11;
const TAG_FOLDERS: u8 = 12;
const TAG_CHOSEN: u8 = 13;
const TAG_TREE_STREAM: u8 = 14;
const TAG_SHORT: u8 = 15;
const TAG_SHORT_CONFIRM: u8 = 16;
const TAG_KEPT: u8 = 17;
const TAG_LAYOUT: u8 = 18;
const TAG_TREE_DIGEST: u8 = 19;
const TAG_TREE_SINCE: u8 = 20;
const TAG_WHO: u8 = 21;
const TAG_SHARED: u8 = 22;
const TAG_SHARED_MKDIR: u8 = 23;
const TAG_SHARED_MOVE: u8 = 24;
const TAG_SHARED_DELETE: u8 = 25;
const TAG_UPLOAD: u8 = 26;
const TAG_UPLOAD_DONE: u8 = 27;

/// The most of a file one [`Request::Upload`] carries: a few chunks' worth,
/// small enough that a slow link sends one in seconds.
pub const MAX_PIECE: usize = 4 << 20;

/// The most entries one listing of a phone's folder may hold: a folder of a
/// hundred thousand photos is a phone's; more is not.
const MAX_KEPT: usize = 200_000;

/// The length of one side's SPAKE2 message in the Ed25519 group: a byte
/// saying which side, and a point.
pub const SPAKE_LEN: usize = 33;
/// The most a sealed long code may be: a nonce, the code, and the tag.
const MAX_SEALED: usize = 512;

/// How large a frame of a streamed file list is made: small enough that the
/// receiver holds little of it at a time.
pub const TREE_FRAME_TARGET: usize = 1 << 20;
/// The largest frame accepted.
pub const MAX_TREE_FRAME: usize = 4 << 20;
/// The most a streamed file list may come to: some seven million files. A
/// peer sending more is not describing a library.
pub const MAX_TREE: usize = 1 << 30;

const STATUS_TREE: u8 = 1;
const STATUS_MANIFEST: u8 = 2;
const STATUS_CHUNK: u8 = 3;
const STATUS_NOT_FOUND: u8 = 4;
const STATUS_PAIRED: u8 = 5;
const STATUS_CHANGED: u8 = 6;
const STATUS_NOTED: u8 = 7;
const STATUS_KEY: u8 = 8;
const STATUS_ABOUT: u8 = 9;
const STATUS_MISMATCH: u8 = 10;
const STATUS_DECLINED: u8 = 11;
const STATUS_WELCOME: u8 = 12;
const STATUS_ASKS: u8 = 13;
const STATUS_FOLDERS: u8 = 14;
const STATUS_CHOSEN: u8 = 15;
const STATUS_SHORT: u8 = 16;
const STATUS_SEALED: u8 = 17;
const STATUS_WITHDRAWN: u8 = 18;
const STATUS_KEPT: u8 = 19;
const STATUS_LAYOUT: u8 = 20;
const STATUS_DIGEST: u8 = 21;
const STATUS_TREE_DELTA: u8 = 22;
const STATUS_WHO: u8 = 23;
const STATUS_REFUSED: u8 = 24;
const STATUS_PLACED: u8 = 25;

/// The most folders one answer may list: two levels of a phone's storage come
/// to a few hundred. A peer claiming more is not describing a phone.
const MAX_FOLDERS: usize = 4096;

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
            Request::Pair { token, device_id, name, kind, key_check } => {
                out.push(TAG_PAIR);
                out.extend_from_slice(token);
                out.extend_from_slice(device_id);
                put_name(&mut out, name);
                put_name(&mut out, kind);
                out.extend_from_slice(key_check);
            }
            Request::Asks => out.push(TAG_ASKS),
            Request::Unlock { nonce, key } => {
                out.push(TAG_UNLOCK);
                out.extend_from_slice(nonce);
                out.extend_from_slice(key);
            }
            Request::Visit { token, device_id, name, kind, person } => {
                out.push(TAG_VISIT);
                out.extend_from_slice(token);
                out.extend_from_slice(device_id);
                put_name(&mut out, name);
                put_name(&mut out, kind);
                out.extend_from_slice(person);
            }
            Request::Join { token, name, kind } => {
                out.push(TAG_JOIN);
                out.extend_from_slice(token);
                put_name(&mut out, name);
                put_name(&mut out, kind);
            }
            Request::About => out.push(TAG_ABOUT),
            Request::Folders => out.push(TAG_FOLDERS),
            Request::Chosen => out.push(TAG_CHOSEN),
            Request::TreeStream => out.push(TAG_TREE_STREAM),
            Request::Short { spake } => {
                out.push(TAG_SHORT);
                out.extend_from_slice(spake);
            }
            Request::ShortConfirm { confirm } => {
                out.push(TAG_SHORT_CONFIRM);
                out.extend_from_slice(confirm);
            }
            Request::Kept { dir } => {
                out.push(TAG_KEPT);
                put_path(&mut out, dir);
            }
            Request::Layout { content } => {
                out.push(TAG_LAYOUT);
                out.extend_from_slice(content);
            }
            Request::TreeDigest => out.push(TAG_TREE_DIGEST),
            Request::TreeSince { since } => {
                out.push(TAG_TREE_SINCE);
                out.extend_from_slice(&since.to_le_bytes());
            }
            Request::Who { token } => {
                out.push(TAG_WHO);
                out.extend_from_slice(token);
            }
            Request::Shared { dir } => {
                out.push(TAG_SHARED);
                put_path(&mut out, dir);
            }
            Request::SharedMkdir { path } => {
                out.push(TAG_SHARED_MKDIR);
                put_path(&mut out, path);
            }
            Request::SharedMove { from, to } => {
                out.push(TAG_SHARED_MOVE);
                put_path(&mut out, from);
                put_path(&mut out, to);
            }
            Request::SharedDelete { path } => {
                out.push(TAG_SHARED_DELETE);
                put_path(&mut out, path);
            }
            Request::Upload { id, offset, bytes } => {
                out.push(TAG_UPLOAD);
                out.extend_from_slice(id);
                out.extend_from_slice(&offset.to_le_bytes());
                let bytes = &bytes[..bytes.len().min(MAX_PIECE)];
                put_u32(&mut out, bytes.len());
                out.extend_from_slice(bytes);
            }
            Request::UploadDone { id, path, content } => {
                out.push(TAG_UPLOAD_DONE);
                out.extend_from_slice(id);
                put_path(&mut out, path);
                out.extend_from_slice(content);
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
                Request::Pair {
                    token,
                    device_id: r.hash()?,
                    name: r.name()?,
                    kind: r.name()?,
                    key_check: r.hash()?,
                }
            }
            TAG_JOIN => {
                let mut token = [0u8; 16];
                token.copy_from_slice(r.take(16)?);
                Request::Join { token, name: r.name()?, kind: r.name()? }
            }
            TAG_ABOUT => Request::About,
            TAG_FOLDERS => Request::Folders,
            TAG_CHOSEN => Request::Chosen,
            TAG_TREE_STREAM => Request::TreeStream,
            TAG_SHORT => Request::Short { spake: r.array()? },
            TAG_SHORT_CONFIRM => Request::ShortConfirm { confirm: r.hash()? },
            TAG_KEPT => Request::Kept { dir: r.path()? },
            TAG_LAYOUT => Request::Layout { content: r.hash()? },
            TAG_TREE_DIGEST => Request::TreeDigest,
            TAG_TREE_SINCE => Request::TreeSince { since: r.u64()? },
            TAG_WHO => {
                let mut token = [0u8; 16];
                token.copy_from_slice(r.take(16)?);
                Request::Who { token }
            }
            TAG_SHARED => Request::Shared { dir: r.path()? },
            TAG_SHARED_MKDIR => Request::SharedMkdir { path: r.path()? },
            TAG_SHARED_MOVE => Request::SharedMove { from: r.path()?, to: r.path()? },
            TAG_SHARED_DELETE => Request::SharedDelete { path: r.path()? },
            TAG_UPLOAD => {
                let mut id = [0u8; 16];
                id.copy_from_slice(r.take(16)?);
                let offset = r.u64()?;
                let len = r.count()?;
                if len > MAX_PIECE {
                    return Err(Error::Protocol { detail: format!("a piece of {len} bytes") });
                }
                Request::Upload { id, offset, bytes: r.take(len)?.to_vec() }
            }
            TAG_UPLOAD_DONE => {
                let mut id = [0u8; 16];
                id.copy_from_slice(r.take(16)?);
                Request::UploadDone { id, path: r.path()?, content: r.hash()? }
            }
            TAG_ASKS => Request::Asks,
            TAG_UNLOCK => {
                let mut nonce = [0u8; 16];
                nonce.copy_from_slice(r.take(16)?);
                Request::Unlock { nonce, key: r.hash()? }
            }
            TAG_VISIT => {
                let mut token = [0u8; 16];
                token.copy_from_slice(r.take(16)?);
                Request::Visit {
                    token,
                    device_id: r.hash()?,
                    name: r.name()?,
                    kind: r.name()?,
                    person: r.hash()?,
                }
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
            Response::Short { spake, confirm } => {
                out.push(STATUS_SHORT);
                out.extend_from_slice(spake);
                out.extend_from_slice(confirm);
            }
            Response::Sealed(sealed) => {
                out.push(STATUS_SEALED);
                put_u32(&mut out, sealed.len());
                out.extend_from_slice(sealed);
            }
            Response::Withdrawn => out.push(STATUS_WITHDRAWN),
            Response::Layout(chunks) => {
                out.push(STATUS_LAYOUT);
                put_u32(&mut out, chunks.len());
                for (hash, size) in chunks {
                    out.extend_from_slice(hash);
                    out.extend_from_slice(&size.to_le_bytes());
                }
            }
            Response::Digest(digest) => {
                out.push(STATUS_DIGEST);
                out.extend_from_slice(digest);
            }
            Response::TreeDelta { now, digest, changed } => {
                out.push(STATUS_TREE_DELTA);
                out.extend_from_slice(&now.to_le_bytes());
                out.extend_from_slice(digest);
                put_u32(&mut out, changed.len());
                for version in changed {
                    encode_version(&mut out, version);
                }
            }
            Response::Kept { folders, files } => {
                out.push(STATUS_KEPT);
                let folders = &folders[..folders.len().min(MAX_KEPT)];
                put_u32(&mut out, folders.len());
                for folder in folders {
                    put_path(&mut out, folder);
                }
                let files = &files[..files.len().min(MAX_KEPT)];
                put_u32(&mut out, files.len());
                for f in files {
                    put_path(&mut out, &f.name);
                    put_path(&mut out, &f.path);
                    out.extend_from_slice(&f.owner);
                    out.extend_from_slice(&f.content);
                    out.extend_from_slice(&f.size.to_le_bytes());
                    out.extend_from_slice(&f.modified_at.to_le_bytes());
                }
            }
            Response::Noted => out.push(STATUS_NOTED),
            Response::Changed { generation } => {
                out.push(STATUS_CHANGED);
                out.extend_from_slice(&generation.to_le_bytes());
            }
            Response::Folders { folders, choices } => {
                out.push(STATUS_FOLDERS);
                let folders = &folders[..folders.len().min(MAX_FOLDERS)];
                put_u32(&mut out, folders.len());
                for folder in folders {
                    put_name(&mut out, &folder.path);
                    out.extend_from_slice(&folder.files.to_le_bytes());
                    out.extend_from_slice(&folder.bytes.to_le_bytes());
                }
                put_choices(&mut out, choices);
            }
            Response::Chosen(choices) => {
                out.push(STATUS_CHOSEN);
                put_choices(&mut out, choices);
            }
            Response::Paired { device_id, name, kind, key_check } => {
                out.push(STATUS_PAIRED);
                out.extend_from_slice(device_id);
                put_name(&mut out, name);
                put_name(&mut out, kind);
                out.extend_from_slice(key_check);
            }
            Response::About { kind } => {
                out.push(STATUS_ABOUT);
                put_name(&mut out, kind);
            }
            Response::Who { name, kind } => {
                out.push(STATUS_WHO);
                put_name(&mut out, name);
                put_name(&mut out, kind);
            }
            Response::Refused { why } => {
                out.push(STATUS_REFUSED);
                put_path(&mut out, why);
            }
            Response::Placed { path } => {
                out.push(STATUS_PLACED);
                put_path(&mut out, path);
            }
            Response::Asks { opening } => {
                out.push(STATUS_ASKS);
                match opening {
                    Some(nonce) => {
                        out.push(1);
                        out.extend_from_slice(nonce);
                    }
                    None => out.push(0),
                }
            }
            Response::Welcome { device_id, name, kind, meeting } => {
                out.push(STATUS_WELCOME);
                out.extend_from_slice(device_id);
                put_name(&mut out, name);
                put_name(&mut out, kind);
                out.extend_from_slice(meeting);
            }
            Response::Mismatch => out.push(STATUS_MISMATCH),
            Response::Declined => out.push(STATUS_DECLINED),
            Response::Key { key } => {
                out.push(STATUS_KEY);
                out.extend_from_slice(key);
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
            STATUS_PAIRED => Response::Paired {
                device_id: r.hash()?,
                name: r.name()?,
                kind: r.name()?,
                key_check: r.hash()?,
            },
            STATUS_ABOUT => Response::About { kind: r.name()? },
            STATUS_WHO => Response::Who { name: r.name()?, kind: r.name()? },
            STATUS_REFUSED => Response::Refused { why: r.path()? },
            STATUS_PLACED => Response::Placed { path: r.path()? },
            STATUS_ASKS => Response::Asks {
                opening: match r.u8()? {
                    0 => None,
                    _ => {
                        let mut nonce = [0u8; 16];
                        nonce.copy_from_slice(r.take(16)?);
                        Some(nonce)
                    }
                },
            },
            STATUS_WELCOME => Response::Welcome {
                device_id: r.hash()?,
                name: r.name()?,
                kind: r.name()?,
                meeting: r.hash()?,
            },
            STATUS_MISMATCH => Response::Mismatch,
            STATUS_FOLDERS => {
                let count = r.count()?;
                if count > MAX_FOLDERS {
                    return Err(Error::Protocol { detail: format!("{count} folders is not a phone's") });
                }
                let mut folders = Vec::with_capacity(count);
                for _ in 0..count {
                    folders.push(FolderSummary { path: r.name()?, files: r.u64()?, bytes: r.u64()? });
                }
                Response::Folders { folders, choices: read_choices(&mut r)? }
            }
            STATUS_CHOSEN => Response::Chosen(read_choices(&mut r)?),
            STATUS_DECLINED => Response::Declined,
            STATUS_KEY => Response::Key { key: r.hash()? },
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
            STATUS_SHORT => Response::Short { spake: r.array()?, confirm: r.hash()? },
            STATUS_SEALED => {
                let len = r.count()?;
                if len > MAX_SEALED {
                    return Err(Error::Protocol { detail: format!("a sealed code of {len} bytes") });
                }
                Response::Sealed(r.take(len)?.to_vec())
            }
            STATUS_WITHDRAWN => Response::Withdrawn,
            STATUS_LAYOUT => {
                let count = r.count()?;
                let mut chunks = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    let hash = r.hash()?;
                    let mut size = [0u8; 4];
                    size.copy_from_slice(r.take(4)?);
                    chunks.push((hash, u32::from_le_bytes(size)));
                }
                Response::Layout(chunks)
            }
            STATUS_DIGEST => Response::Digest(r.hash()?),
            STATUS_TREE_DELTA => {
                let now = r.u64()?;
                let digest = r.hash()?;
                let count = r.count()?;
                let mut changed = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    changed.push(decode_version(&mut r)?);
                }
                Response::TreeDelta { now, digest, changed }
            }
            STATUS_KEPT => {
                let count = r.count()?;
                if count > MAX_KEPT {
                    return Err(Error::Protocol { detail: format!("{count} folders in one place") });
                }
                let mut folders = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    folders.push(r.path()?);
                }
                let count = r.count()?;
                if count > MAX_KEPT {
                    return Err(Error::Protocol { detail: format!("{count} files in one place") });
                }
                let mut files = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    files.push(KeptEntry {
                        name: r.path()?,
                        path: r.path()?,
                        owner: r.hash()?,
                        content: r.hash()?,
                        size: r.u64()?,
                        modified_at: r.u64()? as i64,
                    });
                }
                Response::Kept { folders, files }
            }
            status => {
                return Err(Error::Protocol { detail: format!("unknown response status {status}") })
            }
        };
        r.finished()?;
        Ok(response)
    }
}

/// Choices of folders to back up: each its folder, whether chosen, and when.
fn put_choices(out: &mut Vec<u8>, choices: &[Choice]) {
    let choices = &choices[..choices.len().min(MAX_FOLDERS)];
    put_u32(out, choices.len());
    for choice in choices {
        put_name(out, &choice.folder);
        out.push(choice.chosen as u8);
        out.extend_from_slice(&choice.changed_at.to_le_bytes());
    }
}

fn read_choices(r: &mut Reader<'_>) -> Result<Vec<Choice>> {
    let count = r.count()?;
    if count > MAX_FOLDERS {
        return Err(Error::Protocol { detail: format!("{count} choices is not a phone's") });
    }
    let mut choices = Vec::with_capacity(count);
    for _ in 0..count {
        choices.push(Choice { folder: r.name()?, chosen: r.u8()? != 0, changed_at: r.u64()? as i64 });
    }
    Ok(choices)
}

/// A file list as frames for [`Request::TreeStream`]: each its length, then a
/// count and the versions, about [`TREE_FRAME_TARGET`] at a time, and an empty
/// frame to end.
pub fn tree_frames(versions: &[FileVersion]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut body = Vec::new();
    let mut count = 0usize;
    let flush = |frames: &mut Vec<Vec<u8>>, body: &mut Vec<u8>, count: &mut usize| {
        let mut frame = Vec::with_capacity(body.len() + 8);
        put_u32(&mut frame, body.len() + 4);
        put_u32(&mut frame, *count);
        frame.append(body);
        frames.push(frame);
        *count = 0;
    };
    for v in versions {
        encode_version(&mut body, v);
        count += 1;
        if body.len() >= TREE_FRAME_TARGET {
            flush(&mut frames, &mut body, &mut count);
        }
    }
    if count > 0 {
        flush(&mut frames, &mut body, &mut count);
    }
    frames.push(0u32.to_le_bytes().to_vec());
    frames
}

/// A file list's fingerprint: the hash of its entries in one order, by where
/// each is listed and then by path ([`listed_order`]), however the list was
/// put together -- sent whole, or rebuilt from what changed (decision 0072).
pub fn tree_digest(versions: &[FileVersion]) -> [u8; 32] {
    let mut sorted: Vec<&FileVersion> = versions.iter().collect();
    sorted.sort_by(|a, b| listed_order(a).cmp(&listed_order(b)));
    let mut hasher = blake3::Hasher::new();
    let mut entry = Vec::with_capacity(256);
    for version in sorted {
        entry.clear();
        encode_version(&mut entry, version);
        hasher.update(&entry);
    }
    *hasher.finalize().as_bytes()
}

/// Where an entry is listed -- 0 the shared area, 1 the asker's vault, 2 the
/// answering device's own -- and its path: what tells one entry of a list
/// from another, and the order a list is fingerprinted in. An entry's area
/// within a vault (sent, or held) is not part of it: a file the other device
/// comes to hold is the same entry, changed.
pub fn listed_order(version: &FileVersion) -> (u8, &str) {
    let class = match version.area {
        qurb_sync::Area::Shared => 0,
        qurb_sync::Area::Sent | qurb_sync::Area::Held => 1,
        qurb_sync::Area::Hold => 2,
    };
    (class, version.path.as_str())
}

/// One frame's versions, its length already read.
pub fn decode_tree_frame(bytes: &[u8]) -> Result<Vec<FileVersion>> {
    let mut r = Reader::new(bytes);
    let count = r.count()?;
    let mut versions = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        versions.push(decode_version(&mut r)?);
    }
    Ok(versions)
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

    // Where this entry belongs. Sent explicitly because the receiver cannot
    // tell from the path, and gets one chance to file it correctly: a private
    // version adopted as shared content would be advertised onward to every
    // other device. See `qurb_sync::Area`.
    out.push(v.area.to_byte());
}

fn decode_version(r: &mut Reader<'_>) -> Result<FileVersion> {
    let len = r.count()?;
    if len > MAX_PATH {
        return Err(Error::Protocol { detail: format!("path of {len} bytes is implausible") });
    }
    let path = std::str::from_utf8(r.take(len)?)
        .map_err(|_| Error::Protocol { detail: "path is not utf-8".into() })?
        .to_string();
    // The receiver joins this onto a directory of its own and writes there, or
    // deletes there. A path that leaves that directory is refused here, at the
    // edge, rather than trusted because the peer authenticated -- a stolen
    // device keeps its identity. The engine checks again before it writes.
    if !qurb_sync::is_safe_path(&path) {
        return Err(Error::Protocol { detail: format!("path {path:?} leaves the folder") });
    }

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
        area: {
            let byte = r.u8()?;
            // Refused rather than guessed at. A kind this build does not know
            // could be anybody's private content, and filing it as anything
            // would be a guess about somebody else's data.
            qurb_sync::Area::from_byte(byte)
                .ok_or_else(|| Error::Protocol { detail: format!("unknown area {byte}") })?
        },
    })
}

/// A path, or a name in one, length-prefixed and bounded as a path.
fn put_path(out: &mut Vec<u8>, path: &str) {
    let bytes = path.as_bytes();
    let capped = &bytes[..bytes.len().min(MAX_PATH)];
    put_u32(out, capped.len());
    out.extend_from_slice(capped);
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
        self.array()
    }

    /// A length-prefixed path of at most [`MAX_PATH`] bytes, valid UTF-8.
    /// Not checked to stay inside a folder: what it names is decided by the
    /// side that uses it.
    fn path(&mut self) -> Result<String> {
        let len = self.count()?;
        if len > MAX_PATH {
            return Err(Error::Protocol { detail: format!("path of {len} bytes is implausible") });
        }
        std::str::from_utf8(self.take(len)?)
            .map(|s| s.to_string())
            .map_err(|_| Error::Protocol { detail: "path is not utf-8".into() })
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
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

    /// The same list fingerprints the same, and any change -- one file's
    /// content, one file fewer -- fingerprints differently (`trees`).
    #[test]
    fn a_file_list_has_one_fingerprint() {
        let versions: Vec<FileVersion> = (0..5_000).map(|i| version(&format!("f{i:05}"))).collect();
        assert_eq!(tree_digest(&versions), tree_digest(&versions.clone()));
        let mut changed = versions.clone();
        changed[2_500] = FileVersion::file(
            "f02500",
            [4; 32],
            1234,
            changed[2_500].vector.clone(),
            DeviceId::from_bytes([9; 32]),
            1_757_462_400,
        );
        assert_ne!(tree_digest(&versions), tree_digest(&changed));
        assert_ne!(tree_digest(&versions), tree_digest(&versions[1..]));
    }

    /// A file list too large for one frame arrives whole and in order, in
    /// frames no larger than the cap, the last one empty (decision 0065).
    #[test]
    fn a_file_list_streams_in_frames() {
        let versions: Vec<FileVersion> = (0..20_000).map(|i| version(&format!("dir{}/file{i:06}.bin", i % 7))).collect();
        let frames = tree_frames(&versions);
        assert!(frames.len() > 2, "{} frames for 20,000 versions", frames.len());
        assert_eq!(frames.last().unwrap(), &vec![0, 0, 0, 0], "ends with an empty frame");
        let mut back = Vec::new();
        for frame in &frames[..frames.len() - 1] {
            let length = u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize;
            assert_eq!(length, frame.len() - 4);
            assert!(length <= MAX_TREE_FRAME, "a frame of {length} bytes");
            back.extend(decode_tree_frame(&frame[4..]).unwrap());
        }
        assert_eq!(back, versions);
        assert_eq!(tree_frames(&[]), vec![vec![0, 0, 0, 0]], "nothing is one empty frame");
    }

    #[test]
    fn requests_round_trip() {
        for r in [
            Request::Tree,
            Request::Manifest { content: [1; 32] },
            Request::Chunk { hash: [2; 32] },
            Request::Join { token: [3; 16], name: "Phone".into(), kind: "phone".into() },
            Request::Who { token: [4; 16] },
            Request::Shared { dir: "Photos/2026".into() },
            Request::SharedMkdir { path: "Photos/2027".into() },
            Request::SharedMove { from: "a.txt".into(), to: "Notes/a.txt".into() },
            Request::SharedDelete { path: "Old".into() },
            Request::Upload { id: [5; 16], offset: 4 << 20, bytes: vec![1, 2, 3] },
            Request::UploadDone { id: [5; 16], path: "Photos/beach.jpg".into(), content: [6; 32] },
            Request::About,
            Request::Folders,
            Request::Chosen,
            Request::TreeStream,
            Request::Pair {
                token: [5; 16],
                device_id: [6; 32],
                name: "Laptop".into(),
                kind: "computer".into(),
                key_check: [7; 32],
            },
            Request::Visit {
                token: [8; 16],
                device_id: [9; 32],
                name: "Ammi's phone".into(),
                kind: "phone".into(),
                person: [10; 32],
            },
            Request::Asks,
            Request::Unlock { nonce: [11; 16], key: [12; 32] },
            Request::Layout { content: [13; 32] },
            Request::TreeDigest,
            Request::TreeSince { since: 1 << 40 },
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
            Response::Key { key: [4; 32] },
            Response::About { kind: "replica".into() },
            Response::Who { name: "saqib".into(), kind: "computer".into() },
            Response::Refused { why: "something is already called Notes".into() },
            Response::Placed { path: "Photos/beach (2).jpg".into() },
            Response::Mismatch,
            Response::Declined,
            Response::Paired { device_id: [1; 32], name: "Phone".into(), kind: "phone".into(), key_check: [2; 32] },
            Response::Welcome { device_id: [3; 32], name: "Laptop".into(), kind: "computer".into(), meeting: [4; 32] },
            Response::Asks { opening: None },
            Response::Asks { opening: Some([5; 16]) },
            Response::Folders {
                folders: vec![
                    FolderSummary { path: "DCIM".into(), files: 1200, bytes: 9_000_000_000 },
                    FolderSummary { path: "DCIM/Camera".into(), files: 1100, bytes: 8_800_000_000 },
                ],
                choices: vec![Choice { folder: "DCIM/Camera".into(), chosen: true, changed_at: 1_791_980_000_123 }],
            },
            Response::Folders { folders: vec![], choices: vec![] },
            Response::Chosen(vec![
                Choice { folder: "DCIM/Camera".into(), chosen: true, changed_at: 1_791_980_000_123 },
                Choice { folder: "Documents".into(), chosen: false, changed_at: 1_791_980_100_456 },
            ]),
            Response::Digest([14; 32]),
            Response::TreeDelta { now: 99, digest: [15; 32], changed: vec![version("a/b.txt")] },
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

    /// A tree naming somewhere outside the folder is refused as a whole. A
    /// peer that sends one is not a working copy of this software, and
    /// nothing else it says is worth acting on.
    #[test]
    fn a_path_that_leaves_the_folder_is_refused() {
        for path in ["../../.bashrc", "/etc/cron.d/qurb", "photos/../../x", ""] {
            let bytes = Response::Tree(vec![version(path)]).encode();
            assert!(Response::decode(&bytes).is_err(), "{path:?} was accepted");
        }
        let bytes = Response::Tree(vec![version("photos/beach.jpg")]).encode();
        assert!(Response::decode(&bytes).is_ok(), "an ordinary path was refused");
    }

    /// Each area survives the wire as itself. Filing one as another is filing
    /// somebody's private file in the wrong place.
    #[test]
    fn every_area_survives_the_wire() {
        use qurb_sync::Area;
        for area in [Area::Shared, Area::Sent, Area::Held, Area::Hold] {
            let bytes = Response::Tree(vec![version("a.txt").in_area(area)]).encode();
            match Response::decode(&bytes).unwrap() {
                Response::Tree(got) => assert_eq!(got[0].area, area),
                other => panic!("{other:?}"),
            }
        }
    }

    /// An area this build does not know is refused, not guessed at.
    #[test]
    fn an_unknown_area_is_refused() {
        let mut bytes = Response::Tree(vec![version("a.txt")]).encode();
        *bytes.last_mut().unwrap() = 9;
        assert!(Response::decode(&bytes).is_err());
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
