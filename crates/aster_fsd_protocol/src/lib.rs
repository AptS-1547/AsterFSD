//! Contracts implemented by every wire-protocol backend.
//!
//! Backends translate bounded frames into protocol-independent commands and
//! encode core events for one recipient dialect. They do not own sessions,
//! authentication, routing or persistence.

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

pub use aster_fsd_model::ProtocolDialect;
use aster_fsd_model::{
    Callsign, ClientPresence, Command, ConnectionId, ErrorCode, Event, SessionPhase,
};
use bytes::Bytes;
use std::net::SocketAddr;
use thiserror::Error;

/// One immutable frame without its transport line delimiter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireFrame(Bytes);

impl WireFrame {
    /// Creates a frame from owned bytes.
    ///
    /// # Errors
    ///
    /// Returns a [`ProtocolErrorKind::Encoding`] error when the bytes contain
    /// CR or LF, because framing is exclusively owned by the transport codec.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ProtocolError> {
        if bytes.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
            return Err(ProtocolError::new(
                ProtocolErrorKind::Encoding,
                "wire frame contains a line delimiter",
            ));
        }
        Ok(Self(Bytes::from(bytes)))
    }

    /// Creates a UTF-8 wire frame from text.
    ///
    /// # Errors
    ///
    /// Returns the same delimiter validation error as [`WireFrame::new`].
    pub fn from_text(value: impl Into<String>) -> Result<Self, ProtocolError> {
        Self::new(value.into().into_bytes())
    }

    /// Borrows the encoded bytes without copying.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the frame and returns its reference-counted bytes.
    #[must_use]
    pub fn into_bytes(self) -> Bytes {
        self.0
    }
}

/// Metadata available while constructing server-first handshake frames.
#[derive(Debug, Clone)]
pub struct HandshakeContext {
    pub connection_id: ConnectionId,
    pub peer: SocketAddr,
    pub server_name: String,
    pub server_version: String,
    pub challenge: String,
}

/// Session metadata available while decoding one inbound frame.
#[derive(Debug, Clone)]
pub struct DecodeContext {
    pub connection_id: ConnectionId,
    pub phase: SessionPhase,
    pub callsign: Option<Callsign>,
    pub challenge: String,
}

/// Recipient metadata available while encoding one outbound event.
#[derive(Debug, Clone)]
pub struct EncodeContext {
    pub connection_id: ConnectionId,
    pub recipient: Option<ClientPresence>,
    pub server_name: String,
}

/// Stable category used for logging and wire error mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    Framing,
    Syntax,
    Unsupported,
    InvalidField,
    Encoding,
    Version,
}

/// Structured adapter error, optionally carrying a classic FSD error code.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{message}")]
pub struct ProtocolError {
    pub kind: ProtocolErrorKind,
    pub message: String,
    pub error_code: Option<ErrorCode>,
}

impl ProtocolError {
    /// Creates an adapter error without a protocol error-code override.
    #[must_use]
    pub fn new(kind: ProtocolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            error_code: None,
        }
    }

    /// Associates a classic-compatible error code with this adapter error.
    #[must_use]
    pub fn with_error_code(mut self, error_code: ErrorCode) -> Self {
        self.error_code = Some(error_code);
        self
    }
}

/// Stateless adapter between one wire dialect and the shared domain model.
pub trait ProtocolBackend: Send + Sync {
    /// Returns the unique dialect implemented by this backend.
    fn dialect(&self) -> ProtocolDialect;

    /// Builds frames sent immediately after accepting a connection.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] when the backend cannot encode its handshake.
    fn initial_frames(&self, context: &HandshakeContext) -> Result<Vec<WireFrame>, ProtocolError>;

    /// Decodes one complete inbound frame into a semantic command.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] for malformed, unsupported or phase-invalid
    /// wire input. The server maps the structured category and error code.
    fn decode(&self, context: &DecodeContext, frame: &[u8]) -> Result<Command, ProtocolError>;

    /// Encodes one core event for the supplied recipient context.
    ///
    /// # Errors
    ///
    /// Returns [`ProtocolError`] when the event has no valid representation in
    /// this dialect or would violate the backend's wire constraints.
    fn encode(
        &self,
        context: &EncodeContext,
        event: &Event,
    ) -> Result<Vec<WireFrame>, ProtocolError>;

    /// Indicates whether an event must be re-encoded for every recipient.
    ///
    /// Returning `false` permits the server to encode once per dialect and
    /// share immutable frames among all matching recipients.
    fn encoding_is_recipient_specific(&self, _event: &Event) -> bool {
        false
    }
}
