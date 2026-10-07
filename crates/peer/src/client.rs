//! Asking another device for what it has.

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use crate::tls;
use crate::wire::{Request, Response, MAX_MESSAGE};
use futures_util::stream::{FuturesOrdered, StreamExt};
use qurb_storage::Store;
use qurb_sync::FileVersion;
use std::net::SocketAddr;
use std::path::Path;

/// Chunk requests a fetch keeps in flight at once.
///
/// One at a time, each request waited out a round trip and the other device's
/// reading before the next was asked for, so a file moved at the pace of the
/// round trip rather than of the link: an 800 MB video from a phone on home
/// Wi-Fi went at under 5.3 MB/s. Several overlap those waits, at the cost of
/// holding that many chunks in memory -- 4 MiB at the average chunk size, 16
/// MiB at the largest, and the phone's ceiling (decision 0018) is far above
/// either.
///
/// Measured afterwards, it bought little: from the same phone, about 5.3 MB/s
/// with eight against 3.8 to 4.8 one at a time (2026-10-05, decision 0050).
/// The waits were not what held it back; `PeerClient::report_fetch` logs
/// what might be.
pub const IN_FLIGHT: usize = 8;

/// The smallest fetch worth a line in the log with its rate: below this a
/// transfer is over before its rate means anything.
const REPORT_FROM: u64 = 8 * 1024 * 1024;

/// Where a fetch picks up: what a partly-written file already holds of the
/// content, checked, and the chunks still to come.
pub struct Resume {
    chunks: Vec<[u8; 32]>,
    next: usize,
    whole: blake3::Hasher,
    kept: u64,
}

impl Resume {
    /// Bytes already in place, which the fetch will not ask for again.
    pub fn kept(&self) -> u64 {
        self.kept
    }

    fn from_start(chunks: Vec<[u8; 32]>) -> Self {
        Self { chunks, next: 0, whole: blake3::Hasher::new(), kept: 0 }
    }
}

pub struct PeerClient {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    /// Whether this connection runs through the relay rather than straight to
    /// the other device. Recorded when it is made, because afterwards the two
    /// look the same: the relay presents itself as an ordinary address.
    relayed: bool,
}

impl PeerClient {
    /// Connect to `addr`, refusing to proceed unless the peer proves it holds
    /// the key behind `expected`.
    ///
    /// The fingerprint is required rather than optional. An API that let you
    /// omit it would make "trust whoever answers" the easy path, and that is
    /// the whole attack.
    pub async fn connect(
        addr: SocketAddr,
        identity: &Identity,
        expected: Fingerprint,
    ) -> Result<Self> {
        let bind: SocketAddr = if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }
            .parse()
            .expect("a literal address");
        let mut endpoint = quinn::Endpoint::client(bind)
            .map_err(|e| Error::Io { path: bind.to_string().into(), source: e })?;
        endpoint.set_default_client_config(tls::client_config(identity, expected)?);

        // The name is required by TLS and means nothing here: identity comes
        // from the pinned certificate, not from what the peer calls itself.
        let connection = endpoint.connect(addr, "qurb-device")?.await?;
        Ok(Self { endpoint, connection, relayed: false })
    }

    /// Connect using a socket that has already been used for discovery and
    /// punching.
    ///
    /// The ordinary [`connect`](Self::connect) binds its own socket, which is
    /// fine when the address is directly reachable. It is wrong after hole
    /// punching: the router mapping the peer was told about belongs to the port
    /// that punched it, so a fresh socket arrives at an address nobody expects.
    pub async fn connect_on(
        socket: std::net::UdpSocket,
        addr: SocketAddr,
        identity: &Identity,
        expected: Fingerprint,
    ) -> Result<Self> {
        let mut endpoint = crate::nat::endpoint_from(socket, None)?;
        endpoint.set_default_client_config(tls::client_config(identity, expected)?);
        let connection = endpoint.connect(addr, "qurb-device")?.await?;
        Ok(Self { endpoint, connection, relayed: false })
    }

    /// Wrap a connection someone else established.
    ///
    /// Used by [`Connector`](crate::connect::Connector), which races several
    /// candidate addresses and cannot hand over a connection it has not made
    /// yet.
    pub fn from_parts(endpoint: quinn::Endpoint, connection: quinn::Connection) -> Self {
        Self { endpoint, connection, relayed: false }
    }

    /// Mark a connection as running through the relay.
    pub(crate) fn through_relay(mut self) -> Self {
        self.relayed = true;
        self
    }

    /// Whether the connection has ended, from either side or by going quiet.
    ///
    /// A device that disappears without saying goodbye -- switched off, out of
    /// range -- is noticed by the idle timeout, within half a minute.
    pub fn is_closed(&self) -> bool {
        self.connection.close_reason().is_some()
    }

    /// Whether this connection runs through the relay.
    ///
    /// What a person is told as "through an encrypted relay" rather than
    /// "directly". Either way the session inside is the same end-to-end QUIC
    /// connection with the same pinned identity; only the path differs.
    pub fn is_relayed(&self) -> bool {
        self.relayed
    }

    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Round-trip time, as QUIC currently estimates it.
    pub fn rtt(&self) -> std::time::Duration {
        self.connection.rtt()
    }

    /// Everything the peer knows about, tombstones included.
    pub async fn tree(&self) -> Result<Vec<FileVersion>> {
        match self.request(Request::Tree).await? {
            Response::Tree(versions) => Ok(versions),
            other => Err(unexpected("tree", &other)),
        }
    }

    /// The chunks making up a file, or `None` if the peer does not have it.
    pub async fn manifest(&self, content: [u8; 32]) -> Result<Option<Vec<[u8; 32]>>> {
        match self.request(Request::Manifest { content }).await? {
            Response::Manifest(hashes) => Ok(Some(hashes)),
            Response::NotFound => Ok(None),
            other => Err(unexpected("manifest", &other)),
        }
    }

    pub async fn chunk(&self, hash: [u8; 32]) -> Result<Option<Vec<u8>>> {
        match self.request(Request::Chunk { hash }).await? {
            Response::Chunk(bytes) => Ok(Some(bytes)),
            Response::NotFound => Ok(None),
            other => Err(unexpected("chunk", &other)),
        }
    }

    /// Tell the peer this device now holds that content.
    ///
    /// Courtesy, not bookkeeping this device needs: the *peer* is the one who
    /// learns something, namely that its copy is no longer the only one. That
    /// is what lets a phone say a photo reached the desktop rather than only
    /// that it tried, and what a storage cap consults before dropping a local
    /// copy.
    ///
    /// Sent after the content is committed, never before — a report of a
    /// delivery that then failed is worse than no report, because it is the
    /// evidence someone else may drop their copy on.
    pub async fn got(&self, content: [u8; 32]) -> Result<()> {
        match self.request(Request::Got { content }).await? {
            Response::Noted => Ok(()),
            other => Err(unexpected("acknowledgement", &other)),
        }
    }

    /// What kind of device the peer is -- `phone`, `computer` or `replica` --
    /// for a device paired before devices said so when pairing (decision
    /// 0053). `None` from a device that does not say: an older build answers
    /// "not found".
    pub async fn about(&self) -> Result<Option<String>> {
        match self.request(Request::About).await? {
            Response::About { kind } => Ok(Some(kind)),
            Response::NotFound => Ok(None),
            other => Err(unexpected("description", &other)),
        }
    }

    /// Wait until the peer's state differs from `since`.
    ///
    /// Returns where the peer has got to, both when something changed and when
    /// the wait ran out — either way the answer is what to pass next time.
    ///
    /// This is what turns polling into being told. A device holding this open
    /// costs one idle stream and learns of a change within a round trip,
    /// instead of up to a whole polling interval later.
    pub async fn wait_for_change(&self, since: u64) -> Result<u64> {
        match self.request(Request::Changes { since }).await? {
            Response::Changed { generation } => Ok(generation),
            other => Err(unexpected("change notification", &other)),
        }
    }

    /// Rebuild a file, taking only the chunks this device does not already have.
    ///
    /// This is where the chunking finally pays off across the network. A large
    /// file with a small edit shares almost all of its chunks with the copy
    /// already here, so almost nothing crosses the wire — see
    /// ../../docs/decisions/0004-chunk-parameters.md for the measurement that
    /// justifies the chunk sizes.
    ///
    /// The result is verified against `content` before being returned. A peer
    /// that sends the wrong bytes, or a chunk that was corrupted in transit,
    /// fails here rather than being written to disk.
    pub async fn fetch_content(
        &self,
        local: &Store,
        content: [u8; 32],
        size: u64,
    ) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(size as usize);
        self.fetch_content_into(local, content, &mut out).await?;
        Ok(out)
    }

    /// Rebuild a file, writing it out a chunk at a time.
    ///
    /// The streaming form of [`fetch_content`](Self::fetch_content). Peak memory
    /// is one chunk rather than the whole file, which is the difference between
    /// working and being killed inside an iOS FileProvider extension.
    ///
    /// The whole-file check can only happen after the last byte is written, so
    /// write somewhere temporary and move it once this returns. Content that
    /// failed verification has still been written by then.
    ///
    /// `out` is deliberately not `Send`, so the future this returns is not
    /// `Send` either and cannot be spawned. That is the point: the only caller
    /// writes through a `&mut dyn Write` borrowed from the blocked thread, and
    /// a `Send` bound here would have to be paid for with an `unsafe impl` on
    /// the far side that is sound only while nobody spawns it.
    pub async fn fetch_content_into(
        &self,
        local: &Store,
        content: [u8; 32],
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        let Some(chunks) = self.manifest(content).await? else {
            return Err(Error::ContentUnavailable { hash: blake3::Hash::from(content).to_hex().to_string() });
        };
        self.fetch_rest_into(local, content, Resume::from_start(chunks), out).await
    }

    /// Where to pick up a fetch of `content` into the file at `partial`.
    ///
    /// An 800 MB file pulled from a phone that stopped answering part-way --
    /// its window closed, or the platform froze it -- used to start again from
    /// nothing at the next attempt, and on a phone each attempt gets seconds.
    /// It need not: a partial file is a prefix of the content, and chunk
    /// boundaries are chosen by the bytes, so chunking what is there gives the
    /// chunks the content starts with -- up to the last, which may be cut
    /// short. Those that match the other device's manifest, in order, are kept;
    /// the file is cut back to the end of the last of them, and only the rest
    /// is fetched. Nothing is kept unchecked, so a file left by an attempt at
    /// another version of the same path costs only what does not match.
    pub async fn resume_point(&self, content: [u8; 32], partial: &Path) -> Result<Resume> {
        let Some(chunks) = self.manifest(content).await? else {
            return Err(Error::ContentUnavailable { hash: blake3::Hash::from(content).to_hex().to_string() });
        };
        let io = |e: std::io::Error| Error::Io { path: partial.to_path_buf(), source: e };

        let present = match std::fs::metadata(partial) {
            Ok(meta) if meta.len() > 0 => qurb_storage::chunker::chunk_file(partial)
                .map_err(|e| Error::Protocol { detail: format!("reading the partial file: {e}") })?,
            _ => return Ok(Resume::from_start(chunks)),
        };

        let matching = present
            .chunks
            .iter()
            .zip(&chunks)
            .take_while(|(here, wanted)| here.hash.as_bytes() == *wanted)
            .count();
        let kept: u64 = present.chunks[..matching].iter().map(|c| c.len as u64).sum();

        // Hash what is kept, for the whole-file check at the end, and cut off
        // whatever follows it.
        let mut whole = blake3::Hasher::new();
        if kept > 0 {
            let file = std::fs::File::open(partial).map_err(io)?;
            let mut prefix = std::io::Read::take(file, kept);
            std::io::copy(&mut prefix, &mut whole).map_err(io)?;
        }
        std::fs::OpenOptions::new().write(true).open(partial).and_then(|f| f.set_len(kept)).map_err(io)?;

        Ok(Resume { chunks, next: matching, whole, kept })
    }

    /// Fetch what `resume` says is still missing, writing it to `out` in order.
    ///
    /// Up to [`IN_FLIGHT`] chunks are asked for at once and written in order as
    /// they arrive. Each is checked against its own hash before it is written,
    /// and the whole against `content` after the last.
    pub async fn fetch_rest_into(
        &self,
        local: &Store,
        content: [u8; 32],
        resume: Resume,
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        let started = std::time::Instant::now();
        let kept = resume.kept;
        let mut written = kept;
        let outcome = self.fetch_rest(local, content, resume, out, &mut written).await;
        self.report_fetch(written - kept, started.elapsed(), outcome.is_ok());
        outcome
    }

    /// What a large fetch says about the path it crossed: its rate, and QUIC's
    /// own view of the connection when it ended.
    ///
    /// Recorded because the rate alone did not say enough. A phone serving
    /// over home Wi-Fi gave about 5 MB/s whether chunks were asked for one at
    /// a time or eight at once (2026-10-05), on links of several hundred
    /// megabits. The round trip is the telling figure: a long one points at
    /// the path, or a phone's radio dozing between packets; a short one at the
    /// sender. The congestion window and losses are this side's own, of its
    /// requests, and say whether those were held back.
    fn report_fetch(&self, bytes: u64, took: std::time::Duration, finished: bool) {
        if bytes < REPORT_FROM {
            return;
        }
        let path = self.connection.stats().path;
        tracing::info!(
            bytes,
            seconds = format!("{:.1}", took.as_secs_f64()),
            mb_per_s = format!("{:.2}", bytes as f64 / took.as_secs_f64().max(0.001) / 1e6),
            finished,
            rtt_ms = path.rtt.as_millis() as u64,
            cwnd = path.cwnd,
            lost_packets = path.lost_packets,
            congestion_events = path.congestion_events,
            mtu = path.current_mtu,
            "fetched"
        );
    }

    async fn fetch_rest(
        &self,
        local: &Store,
        content: [u8; 32],
        resume: Resume,
        out: &mut impl std::io::Write,
        written: &mut u64,
    ) -> Result<u64> {
        let hash = blake3::Hash::from(content);
        let Resume { chunks, next, mut whole, .. } = resume;

        let mut wanted = chunks.into_iter().skip(next);
        let mut in_flight = FuturesOrdered::new();
        loop {
            while in_flight.len() < IN_FLIGHT {
                let Some(chunk_hash) = wanted.next() else { break };
                in_flight.push_back(self.chunk_bytes(local, chunk_hash));
            }
            let Some(bytes) = in_flight.next().await else { break };
            let bytes = bytes?;

            whole.update(&bytes);
            out.write_all(&bytes).map_err(|e| Error::Io {
                path: "the destination".into(),
                source: e,
            })?;
            *written += bytes.len() as u64;
        }

        if whole.finalize() != hash {
            return Err(Error::ContentMismatch { hash: hash.to_hex().to_string() });
        }
        Ok(*written)
    }

    /// One chunk's bytes: from this device when it already has them -- with
    /// another file, an earlier version of this one, or a copy under another
    /// name -- and otherwise from the peer, checked against the hash that names
    /// them, which catches both a lying peer and a damaged transfer.
    async fn chunk_bytes(&self, local: &Store, chunk_hash: [u8; 32]) -> Result<Vec<u8>> {
        let chunk = blake3::Hash::from(chunk_hash);
        if local.has_chunk(&chunk)? {
            return Ok(local.read_chunk(&chunk)?);
        }
        let fetched = self
            .chunk(chunk_hash)
            .await?
            .ok_or_else(|| Error::ContentUnavailable { hash: chunk.to_hex().to_string() })?;
        if blake3::hash(&fetched) != chunk {
            return Err(Error::ContentMismatch { hash: chunk.to_hex().to_string() });
        }
        Ok(fetched)
    }

    async fn request(&self, request: Request) -> Result<Response> {
        let (mut send, mut recv) = self.connection.open_bi().await?;
        send.write_all(&request.encode()).await?;
        // Finishing the stream is what tells the peer the request is complete;
        // there is no length prefix because the stream itself is the frame.
        send.finish()?;
        let raw = recv.read_to_end(MAX_MESSAGE).await?;
        Response::decode(&raw)
    }

    pub fn close(&self) {
        self.connection.close(0u32.into(), b"done");
        self.endpoint.close(0u32.into(), b"done");
    }

    /// Close this connection and nothing else.
    ///
    /// [`close`](Self::close) also closes the endpoint, which is right for a
    /// client that owns one and wrong for the connector's, which every other
    /// connection shares: closing that because one device was removed would
    /// take the rest down with it.
    pub fn disconnect(&self, why: &str) {
        self.connection.close(0u32.into(), why.as_bytes());
    }
}

fn unexpected(wanted: &str, got: &Response) -> Error {
    let kind = match got {
        Response::Tree(_) => "tree",
        Response::Manifest(_) => "manifest",
        Response::Chunk(_) => "chunk",
        Response::NotFound => "not-found",
        Response::Noted => "acknowledgement",
        Response::Paired { .. } => "pairing reply",
        Response::Key { .. } => "key",
        Response::About { .. } => "description",
        Response::Mismatch => "key mismatch",
        Response::Declined => "refusal",
        Response::Changed { .. } => "change notification",
    };
    Error::Protocol { detail: format!("asked for a {wanted}, got a {kind}") }
}
