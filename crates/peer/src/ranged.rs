//! Part of a file, read from another device as it is wanted.
//!
//! What a player needs to show a video kept on a computer while it arrives,
//! seeking included, rather than after all of it has. On 2026-10-15 the
//! owner's S23 struggled to play large videos from the laptop: each was
//! fetched whole into the app's cache before it could start, 1.6 GB for one
//! episode, and the cache grew with every one.
//!
//! The other device gives the file's chunks with their sizes
//! ([`PeerClient::layout`]), so where any byte lies is known. A read fetches
//! only the chunks it covers, each checked against its hash, keeps the last
//! few, and fetches a few ahead of a reader going forward. A read returns at
//! most [`MOST`] bytes: a player asks for less, and a caller wanting the whole
//! file has a way that does not hold it in memory (decision 0018).

use crate::client::PeerClient;
use crate::error::{Error, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

/// The most one read returns.
pub const MOST: usize = 1 << 20;

/// Chunks kept after they are read: a player's next reads land in them, and
/// at most 2 MiB each, the bound stays small.
const KEPT: usize = 16;

/// How many chunks past a read are fetched before they are asked for.
const AHEAD: usize = 4;

/// One chunk: fetched once, by whichever read or read-ahead asks first.
type Slot = Arc<OnceCell<Arc<Vec<u8>>>>;

/// A file on another device, to be read in parts.
pub struct Ranged {
    client: PeerClient,
    chunks: Vec<[u8; 32]>,
    /// Where each chunk starts in the file, then the file's size.
    starts: Vec<u64>,
    slots: Mutex<Slots>,
}

#[derive(Default)]
struct Slots {
    by_index: HashMap<usize, Slot>,
    /// Least recently asked for first.
    order: VecDeque<usize>,
}

impl Slots {
    fn take(&mut self, index: usize) -> Slot {
        self.order.retain(|&i| i != index);
        self.order.push_back(index);
        let slot = Arc::clone(self.by_index.entry(index).or_default());
        while self.order.len() > KEPT {
            if let Some(oldest) = self.order.pop_front() {
                self.by_index.remove(&oldest);
            }
        }
        slot
    }
}

impl Ranged {
    /// `content` on the device `client` reaches, or `None` if it does not
    /// have it or will not show it.
    pub async fn open(client: PeerClient, content: [u8; 32]) -> Result<Option<Arc<Self>>> {
        let Some(layout) = client.layout(content).await? else {
            return Ok(None);
        };
        let mut starts = Vec::with_capacity(layout.len() + 1);
        let mut at = 0u64;
        for (_, size) in &layout {
            starts.push(at);
            at += u64::from(*size);
        }
        starts.push(at);
        Ok(Some(Arc::new(Self {
            client,
            chunks: layout.into_iter().map(|(hash, _)| hash).collect(),
            starts,
            slots: Mutex::new(Slots::default()),
        })))
    }

    /// End the connection the file is read over: a read waiting on it fails
    /// at once rather than when the connection times out. What the phone does
    /// when its network changes under a player (2026-10-19).
    pub fn close(&self) {
        self.client.disconnect("stopped reading");
    }

    /// The file's size in bytes.
    pub fn size(&self) -> u64 {
        self.starts.last().copied().unwrap_or(0)
    }

    /// Up to `length` bytes from `offset`, fewer at the end of the file and
    /// none past it; at most [`MOST`].
    pub async fn read_at(self: &Arc<Self>, offset: u64, length: usize) -> Result<Vec<u8>> {
        let end = offset.saturating_add(length.min(MOST) as u64).min(self.size());
        if offset >= end {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity((end - offset) as usize);
        let mut index = self.index_of(offset);
        while index < self.chunks.len() && self.starts[index] < end {
            let bytes = self.chunk(index).await?;
            let from = offset.saturating_sub(self.starts[index]) as usize;
            let to = ((end - self.starts[index]) as usize).min(bytes.len());
            out.extend_from_slice(&bytes[from.min(to)..to]);
            index += 1;
        }
        // Ahead of a reader going forward: a player plays on from here.
        for ahead in index..(index + AHEAD).min(self.chunks.len()) {
            let ranged = Arc::clone(self);
            tokio::spawn(async move {
                let _ = ranged.chunk(ahead).await;
            });
        }
        Ok(out)
    }

    /// The chunk `offset` falls in.
    fn index_of(&self, offset: u64) -> usize {
        match self.starts[..self.chunks.len()].binary_search(&offset) {
            Ok(index) => index,
            Err(after) => after.saturating_sub(1),
        }
    }

    async fn chunk(&self, index: usize) -> Result<Arc<Vec<u8>>> {
        let slot = self.slots.lock().map_err(|_| Error::Protocol { detail: "a reader failed".into() })?.take(index);
        let hash = self.chunks[index];
        slot.get_or_try_init(|| async {
            let chunk = blake3::Hash::from(hash);
            let bytes = self
                .client
                .chunk(hash)
                .await?
                .ok_or_else(|| Error::ContentUnavailable { hash: chunk.to_hex().to_string() })?;
            if blake3::hash(&bytes) != chunk {
                return Err(Error::ContentMismatch { hash: chunk.to_hex().to_string() });
            }
            Ok(Arc::new(bytes))
        })
        .await
        .cloned()
    }
}
