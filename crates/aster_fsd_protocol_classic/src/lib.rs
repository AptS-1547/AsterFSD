//! Classic FSD draft 9 protocol backend.
//!
//! This adapter mirrors the original C server's command surface while mapping
//! every packet into shared domain commands and events. It is intentionally
//! stateless: session authority, source ownership and routing live in the core.

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

mod decode;
mod encode;

use aster_fsd_model::{Command, Event};
use aster_fsd_protocol::{
    DecodeContext, EncodeContext, HandshakeContext, ProtocolBackend, ProtocolDialect,
    ProtocolError, WireFrame,
};

/// Stateless classic FSD draft 9 wire adapter.
#[derive(Debug, Default)]
pub struct ClassicProtocol;

impl ProtocolBackend for ClassicProtocol {
    fn dialect(&self) -> ProtocolDialect {
        ProtocolDialect::Classic
    }

    fn initial_frames(&self, _context: &HandshakeContext) -> Result<Vec<WireFrame>, ProtocolError> {
        Ok(Vec::new())
    }

    fn decode(&self, _context: &DecodeContext, frame: &[u8]) -> Result<Command, ProtocolError> {
        decode::decode(frame)
    }

    fn encode(
        &self,
        context: &EncodeContext,
        event: &Event,
    ) -> Result<Vec<WireFrame>, ProtocolError> {
        encode::encode(context, event)
    }
}

#[cfg(test)]
mod tests;
