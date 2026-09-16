//! Asking another device for what it has.

use crate::error::{Error, Result};
use crate::identity::{Fingerprint, Identity};
use crate::tls;
use crate::wire::{Request, Response, MAX_MESSAGE};
use qurb_storage::Store;
use qurb_sync::FileVersion;
use std::net::SocketAddr;

pub struct PeerClient {
    endpoint: quinn::Endpoint,
    connection: quinn::Connection,
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
        Ok(Self { endpoint, connection })
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
        let hash = blake3::Hash::from(content);
        let Some(chunks) = self.manifest(content).await? else {
            return Err(Error::ContentUnavailable { hash: hash.to_hex().to_string() });
        };

        let mut out = Vec::with_capacity(size as usize);
        for chunk_hash in chunks {
            let chunk = blake3::Hash::from(chunk_hash);

            // Already here? Then it does not need to cross the network, whether
            // it arrived with another file, an earlier version of this one, or
            // a copy under a different name.
            let bytes = if local.has_chunk(&chunk)? {
                local.read_chunk(&chunk)?
            } else {
                let fetched = self
                    .chunk(chunk_hash)
                    .await?
                    .ok_or_else(|| Error::ContentUnavailable {
                        hash: chunk.to_hex().to_string(),
                    })?;
                // Chunks are named by their content, so this is free to check
                // and catches both a lying peer and a damaged transfer.
                if blake3::hash(&fetched) != chunk {
                    return Err(Error::ContentMismatch { hash: chunk.to_hex().to_string() });
                }
                fetched
            };
            out.extend_from_slice(&bytes);
        }

        if blake3::hash(&out) != hash {
            return Err(Error::ContentMismatch { hash: hash.to_hex().to_string() });
        }
        Ok(out)
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
}

fn unexpected(wanted: &str, got: &Response) -> Error {
    let kind = match got {
        Response::Tree(_) => "tree",
        Response::Manifest(_) => "manifest",
        Response::Chunk(_) => "chunk",
        Response::NotFound => "not-found",
    };
    Error::Protocol { detail: format!("asked for a {wanted}, got a {kind}") }
}
