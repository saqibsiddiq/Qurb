//! The relay's wire format.
//!
//! Binary and minimal, because every frame wraps a QUIC packet and the overhead
//! is paid on every byte that could not take a direct path. The signalling
//! channel uses JSON for readability; here the trade runs the other way.
//!
//! ```text
//!   [u32 length][u8 tag][body]
//! ```

use crate::error::{Error, Result};

/// Frames larger than this are refused. A QUIC datagram is around 1200 bytes;
/// anything near this cap is already wrong, and without a cap a four-byte
/// length field is an out-of-memory attack.
pub const MAX_FRAME: usize = 64 * 1024;

const TAG_REGISTER: u8 = 1;
const TAG_FORWARD: u8 = 2;
const TAG_DELIVER: u8 = 3;

/// Who a device says it is, on the relay.
///
/// The same identifier the rendezvous service uses: derived from the user's
/// master key, so the relay learns nothing about whose traffic it carries.
pub type RelayId = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// I am here, under this identifier.
    Register { member: RelayId },
    /// Pass this to that identifier.
    Forward { to: RelayId, payload: Vec<u8> },
    /// Something arrived for you.
    Deliver { from: RelayId, payload: Vec<u8> },
}

impl Frame {
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(64);
        match self {
            Frame::Register { member } => {
                body.push(TAG_REGISTER);
                body.extend_from_slice(member);
            }
            Frame::Forward { to, payload } => {
                body.push(TAG_FORWARD);
                body.extend_from_slice(to);
                body.extend_from_slice(payload);
            }
            Frame::Deliver { from, payload } => {
                body.push(TAG_DELIVER);
                body.extend_from_slice(from);
                body.extend_from_slice(payload);
            }
        }

        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// Decode a frame body, the length prefix already removed.
    pub fn decode(body: &[u8]) -> Result<Self> {
        let (&tag, rest) = body.split_first().ok_or(Error::Malformed("empty frame"))?;

        match tag {
            TAG_REGISTER => Ok(Frame::Register { member: take_id(rest)?.0 }),
            TAG_FORWARD => {
                let (to, payload) = take_id(rest)?;
                Ok(Frame::Forward { to, payload: payload.to_vec() })
            }
            TAG_DELIVER => {
                let (from, payload) = take_id(rest)?;
                Ok(Frame::Deliver { from, payload: payload.to_vec() })
            }
            _ => Err(Error::Malformed("unknown frame tag")),
        }
    }
}

fn take_id(bytes: &[u8]) -> Result<(RelayId, &[u8])> {
    if bytes.len() < 32 {
        return Err(Error::Malformed("frame is too short for an identifier"));
    }
    let mut id = [0u8; 32];
    id.copy_from_slice(&bytes[..32]);
    Ok((id, &bytes[32..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let frames = [
            Frame::Register { member: [1; 32] },
            Frame::Forward { to: [2; 32], payload: vec![0xAB; 1200] },
            Frame::Deliver { from: [3; 32], payload: Vec::new() },
        ];
        for frame in frames {
            let encoded = frame.encode();
            let length =
                u32::from_be_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]) as usize;
            assert_eq!(length, encoded.len() - 4, "length prefix disagrees with the body");
            assert_eq!(Frame::decode(&encoded[4..]).unwrap(), frame);
        }
    }

    #[test]
    fn truncation_is_refused_at_every_length() {
        let encoded = Frame::Forward { to: [7; 32], payload: vec![9; 40] }.encode();
        let body = &encoded[4..];
        for cut in 0..body.len() {
            // A short frame must fail or decode to something smaller -- never to
            // the original.
            if let Ok(decoded) = Frame::decode(&body[..cut]) {
                assert_ne!(
                    decoded,
                    Frame::Forward { to: [7; 32], payload: vec![9; 40] },
                    "a truncated frame decoded to the full one"
                );
            }
        }
    }

    #[test]
    fn rubbish_is_refused() {
        assert!(Frame::decode(&[]).is_err());
        assert!(Frame::decode(&[99]).is_err());
        assert!(Frame::decode(&[TAG_FORWARD, 1, 2, 3]).is_err());
    }

    #[test]
    fn an_empty_payload_is_allowed() {
        // QUIC may send padding-only packets, and refusing them would break a
        // connection for no reason.
        let frame = Frame::Forward { to: [1; 32], payload: Vec::new() };
        assert_eq!(Frame::decode(&frame.encode()[4..]).unwrap(), frame);
    }
}
