//! Bounded line framing and raw classic FSD packet tokenization.
//!
//! The codec enforces byte limits before a connection buffer can grow without
//! bound, accepts CR, LF and CRLF delimiters, and preserves payload bytes after
//! tokenization. Semantic command validation belongs to protocol backends.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io;
use thiserror::Error;
use tokio_util::codec::{Decoder, Encoder};

/// Maximum classic FSD draft 9 frame size, excluding line delimiters.
pub const CLASSIC_MAX_FRAME_BYTES: usize = 511;

/// Framing and raw packet tokenization failures.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("frame exceeds configured limit of {limit} bytes")]
    FrameTooLong { limit: usize },
    #[error("frame contains invalid UTF-8")]
    InvalidUtf8,
    #[error("frame is empty")]
    Empty,
    #[error("unknown packet prefix {0:#04x}")]
    UnknownPrefix(u8),
    #[error("packet is missing {0}")]
    Missing(&'static str),
    #[error("packet command is invalid")]
    InvalidCommand,
    #[error("encoded frame contains a line delimiter")]
    EmbeddedDelimiter,
}

impl From<CodecError> for io::Error {
    fn from(error: CodecError) -> Self {
        let kind = match error {
            CodecError::FrameTooLong { .. } => io::ErrorKind::InvalidData,
            _ => io::ErrorKind::InvalidInput,
        };
        Self::new(kind, error)
    }
}

/// Tokio codec for a bounded delimiter-terminated FSD byte stream.
#[derive(Debug, Clone)]
pub struct FsdFrameCodec {
    max_frame_bytes: usize,
}

impl FsdFrameCodec {
    /// Creates a codec with an explicit frame byte limit.
    ///
    /// # Errors
    ///
    /// Returns [`CodecError::FrameTooLong`] when `max_frame_bytes` is zero;
    /// a zero-byte limit cannot accept any valid FSD frame.
    pub fn new(max_frame_bytes: usize) -> Result<Self, CodecError> {
        if max_frame_bytes == 0 {
            return Err(CodecError::FrameTooLong { limit: 0 });
        }
        Ok(Self { max_frame_bytes })
    }

    /// Returns the configured maximum frame size, excluding delimiters.
    #[must_use]
    pub fn max_frame_bytes(&self) -> usize {
        self.max_frame_bytes
    }

    fn delimiter_index(source: &BytesMut) -> Option<usize> {
        source.iter().position(|byte| matches!(byte, b'\r' | b'\n'))
    }

    fn discard_delimiters(source: &mut BytesMut) {
        let count = source
            .iter()
            .take_while(|byte| matches!(byte, b'\r' | b'\n'))
            .count();
        source.advance(count);
    }
}

impl Decoder for FsdFrameCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, source: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        Self::discard_delimiters(source);
        if source.is_empty() {
            return Ok(None);
        }

        if let Some(index) = Self::delimiter_index(source) {
            if index > self.max_frame_bytes {
                source.advance(index);
                Self::discard_delimiters(source);
                return Err(CodecError::FrameTooLong {
                    limit: self.max_frame_bytes,
                }
                .into());
            }
            let frame = source.split_to(index).to_vec();
            Self::discard_delimiters(source);
            if frame.is_empty() {
                return self.decode(source);
            }
            return Ok(Some(frame));
        }

        if source.len() > self.max_frame_bytes {
            source.clear();
            return Err(CodecError::FrameTooLong {
                limit: self.max_frame_bytes,
            }
            .into());
        }

        source.reserve(self.max_frame_bytes.saturating_sub(source.len()).min(1024));
        Ok(None)
    }
}

impl Encoder<Vec<u8>> for FsdFrameCodec {
    type Error = io::Error;

    fn encode(&mut self, frame: Vec<u8>, destination: &mut BytesMut) -> Result<(), Self::Error> {
        if frame.len() > self.max_frame_bytes {
            return Err(CodecError::FrameTooLong {
                limit: self.max_frame_bytes,
            }
            .into());
        }
        if frame.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return Err(CodecError::EmbeddedDelimiter.into());
        }
        destination.reserve(frame.len() + 2);
        destination.put_slice(&frame);
        destination.put_slice(b"\r\n");
        Ok(())
    }
}

impl Encoder<Bytes> for FsdFrameCodec {
    type Error = io::Error;

    fn encode(&mut self, frame: Bytes, destination: &mut BytesMut) -> Result<(), Self::Error> {
        if frame.len() > self.max_frame_bytes {
            return Err(CodecError::FrameTooLong {
                limit: self.max_frame_bytes,
            }
            .into());
        }
        if frame.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return Err(CodecError::EmbeddedDelimiter.into());
        }
        destination.reserve(frame.len() + 2);
        destination.put_slice(&frame);
        destination.put_slice(b"\r\n");
        Ok(())
    }
}

/// Classic FSD packet family selected by the first wire byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawPacketKind {
    Request,
    Client,
    PilotPosition,
    AtcPosition,
    IvaoSpecific,
    IvaoData,
    IvaoOther,
}

impl RawPacketKind {
    fn from_prefix(prefix: u8) -> Result<Self, CodecError> {
        match prefix {
            b'$' => Ok(Self::Request),
            b'#' => Ok(Self::Client),
            b'@' => Ok(Self::PilotPosition),
            b'%' => Ok(Self::AtcPosition),
            b'!' => Ok(Self::IvaoSpecific),
            b'&' => Ok(Self::IvaoData),
            b'-' => Ok(Self::IvaoOther),
            value => Err(CodecError::UnknownPrefix(value)),
        }
    }

    /// Returns the classic FSD prefix byte for this packet family.
    #[must_use]
    pub fn prefix(self) -> u8 {
        match self {
            Self::Request => b'$',
            Self::Client => b'#',
            Self::PilotPosition => b'@',
            Self::AtcPosition => b'%',
            Self::IvaoSpecific => b'!',
            Self::IvaoData => b'&',
            Self::IvaoOther => b'-',
        }
    }
}

/// Lossless tokenized representation of a classic FSD frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPacket {
    pub kind: RawPacketKind,
    pub command: String,
    pub source: String,
    pub destination: String,
    pub fields: Vec<String>,
}

impl RawPacket {
    /// Tokenizes one complete frame without its CR/LF delimiter.
    ///
    /// # Errors
    ///
    /// Returns [`CodecError`] when the frame is empty, is not UTF-8, uses an
    /// unknown prefix, or omits fields required to determine packet routing.
    pub fn parse(frame: &[u8]) -> Result<Self, CodecError> {
        if frame.is_empty() {
            return Err(CodecError::Empty);
        }
        let kind = RawPacketKind::from_prefix(frame[0])?;
        let body = std::str::from_utf8(&frame[1..]).map_err(|_| CodecError::InvalidUtf8)?;

        match kind {
            RawPacketKind::PilotPosition => Self::parse_pilot(body),
            RawPacketKind::AtcPosition => Self::parse_atc(body),
            _ => Self::parse_command(kind, body),
        }
    }

    fn parse_pilot(body: &str) -> Result<Self, CodecError> {
        let mut fields = body.split(':');
        let mode_and_callsign = fields.next().ok_or(CodecError::Missing("pilot mode"))?;
        let mut chars = mode_and_callsign.chars();
        let mode = chars.next().ok_or(CodecError::Missing("pilot mode"))?;
        if !matches!(mode, 'N' | 'S' | 'Y') {
            return Err(CodecError::InvalidCommand);
        }
        let compact_callsign = chars.as_str();
        let source = if compact_callsign.is_empty() {
            fields
                .next()
                .filter(|value| !value.is_empty())
                .ok_or(CodecError::Missing("pilot callsign"))?
        } else {
            compact_callsign
        };
        Ok(Self {
            kind: RawPacketKind::PilotPosition,
            command: mode.to_string(),
            source: source.to_string(),
            destination: String::new(),
            fields: fields.map(str::to_string).collect(),
        })
    }

    fn parse_atc(body: &str) -> Result<Self, CodecError> {
        let mut fields = body.split(':');
        let source = fields
            .next()
            .filter(|value| !value.is_empty())
            .ok_or(CodecError::Missing("ATC callsign"))?;
        Ok(Self {
            kind: RawPacketKind::AtcPosition,
            command: String::new(),
            source: source.to_string(),
            destination: String::new(),
            fields: fields.map(str::to_string).collect(),
        })
    }

    fn parse_command(kind: RawPacketKind, body: &str) -> Result<Self, CodecError> {
        let (head, rest) = body
            .split_once(':')
            .ok_or(CodecError::Missing("destination separator"))?;
        if head.len() < 2 || !head.is_ascii() {
            return Err(CodecError::InvalidCommand);
        }
        let command = &head[..2];
        let first_identifier = &head[2..];
        let mut rest = rest.split(':');
        let second_identifier = rest.next().ok_or(CodecError::Missing("destination"))?;
        let (source, destination) = if command == "DI" {
            (second_identifier, first_identifier)
        } else {
            (first_identifier, second_identifier)
        };
        Ok(Self {
            kind,
            command: command.to_string(),
            source: source.to_string(),
            destination: destination.to_string(),
            fields: rest.map(str::to_string).collect(),
        })
    }

    /// Reassembles the tokenized packet without adding a line delimiter.
    ///
    /// # Errors
    ///
    /// Returns [`CodecError::FrameTooLong`] when the encoded bytes exceed the
    /// supplied limit, or [`CodecError::EmbeddedDelimiter`] when any token
    /// contains CR or LF.
    pub fn encode(&self, max_frame_bytes: usize) -> Result<Vec<u8>, CodecError> {
        let mut output = Vec::new();
        output.push(self.kind.prefix());
        match self.kind {
            RawPacketKind::PilotPosition => {
                output.extend_from_slice(self.command.as_bytes());
                output.push(b':');
                output.extend_from_slice(self.source.as_bytes());
            }
            RawPacketKind::AtcPosition => output.extend_from_slice(self.source.as_bytes()),
            _ if self.command == "DI" => {
                output.extend_from_slice(self.command.as_bytes());
                output.extend_from_slice(self.destination.as_bytes());
                output.push(b':');
                output.extend_from_slice(self.source.as_bytes());
            }
            _ => {
                output.extend_from_slice(self.command.as_bytes());
                output.extend_from_slice(self.source.as_bytes());
                output.push(b':');
                output.extend_from_slice(self.destination.as_bytes());
            }
        }
        for field in &self.fields {
            output.push(b':');
            output.extend_from_slice(field.as_bytes());
        }
        if output.len() > max_frame_bytes {
            return Err(CodecError::FrameTooLong {
                limit: max_frame_bytes,
            });
        }
        if output.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return Err(CodecError::EmbeddedDelimiter);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_accepts_cr_lf_and_crlf() {
        let mut codec = FsdFrameCodec::new(32).unwrap();
        let mut source = BytesMut::from(&b"one\rtwo\nthree\r\n"[..]);
        assert_eq!(codec.decode(&mut source).unwrap(), Some(b"one".to_vec()));
        assert_eq!(codec.decode(&mut source).unwrap(), Some(b"two".to_vec()));
        assert_eq!(codec.decode(&mut source).unwrap(), Some(b"three".to_vec()));
        assert_eq!(codec.decode(&mut source).unwrap(), None);
    }

    #[test]
    fn decoder_rejects_before_unbounded_growth() {
        let mut codec = FsdFrameCodec::new(4).unwrap();
        let mut source = BytesMut::from(&b"12345"[..]);
        assert_eq!(
            codec.decode(&mut source).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(source.is_empty());
    }

    #[test]
    fn classic_packet_round_trip_preserves_payload_spaces() {
        let frame = b"#TMECP1:ECP2:hello  ";
        let packet = RawPacket::parse(frame).unwrap();
        assert_eq!(packet.fields, vec!["hello  "]);
        assert_eq!(packet.encode(CLASSIC_MAX_FRAME_BYTES).unwrap(), frame);
    }

    #[test]
    fn pilot_position_supports_standard_and_compact_input() {
        for frame in [
            b"@S:ECP1:1200:1:1:2:3:4:5:6".as_slice(),
            b"@SECP1:1200:1:1:2:3:4:5:6",
        ] {
            let packet = RawPacket::parse(frame).unwrap();
            assert_eq!(packet.command, "S");
            assert_eq!(packet.source, "ECP1");
        }
    }

    #[test]
    fn frame_limits_cover_zero_exact_and_overflow_boundaries() {
        assert!(matches!(
            FsdFrameCodec::new(0),
            Err(CodecError::FrameTooLong { limit: 0 })
        ));

        let mut codec = FsdFrameCodec::new(4).unwrap();
        let mut destination = BytesMut::new();
        codec.encode(b"1234".to_vec(), &mut destination).unwrap();
        assert_eq!(destination.as_ref(), b"1234\r\n");

        let error = codec
            .encode(b"12345".to_vec(), &mut BytesMut::new())
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn encoder_rejects_embedded_delimiters_without_partial_output() {
        let mut codec = FsdFrameCodec::new(32).unwrap();
        for frame in [b"one\rtwo".to_vec(), b"one\ntwo".to_vec()] {
            let mut destination = BytesMut::new();
            let error = codec.encode(frame, &mut destination).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert!(destination.is_empty());
        }
    }

    #[test]
    fn raw_packet_rejects_invalid_utf8_prefix_and_missing_destination() {
        assert!(matches!(
            RawPacket::parse(&[b'#', 0xff]),
            Err(CodecError::InvalidUtf8)
        ));
        assert!(matches!(
            RawPacket::parse(b"?TMONE:TWO"),
            Err(CodecError::UnknownPrefix(b'?'))
        ));
        assert!(matches!(
            RawPacket::parse(b"#TMONE"),
            Err(CodecError::Missing("destination separator"))
        ));
    }
}
