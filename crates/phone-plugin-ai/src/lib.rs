use phone_core::Classification;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Purpose {
    Screening,
    ReadAloud,
}
#[derive(Debug)]
pub enum Notice {
    Route(Classification),
    Transcript {
        speaker: String,
        item_id: String,
        text: String,
    },
    Truncate {
        item_id: String,
    },
    Failed,
}
#[derive(Debug)]
pub struct Report {
    pub call_id: Uuid,
    pub generation: Uuid,
    pub notice: Notice,
}
pub struct BridgeContext {
    pub call_id: Uuid,
    pub generation: Uuid,
    pub purpose: Purpose,
    pub model: String,
    pub transcription_model: String,
}
#[derive(Debug, thiserror::Error)]
pub enum StartupFault {
    #[error("media_channel_create_failed")]
    Create,
    #[error("media_connection_identifier_failed")]
    Identifier,
    #[error("media_websocket_connect_failed")]
    Connect,
    #[error("media_bridge_attach_failed")]
    Attach,
    #[error("media_stasis_readiness_failed")]
    Stasis,
    #[error("media_start_read_failed")]
    Read,
    #[error("media_start_was_not_text")]
    Text,
    #[error("media_start_decode_failed")]
    Decode,
    #[error("media_start_format_failed")]
    Format,
}

#[cfg(feature = "runtime")]
mod runtime;
#[cfg(feature = "runtime")]
pub use runtime::*;
#[cfg(feature = "runtime")]
mod http_body;
#[cfg(feature = "runtime")]
pub mod inference;

#[cfg(feature = "runtime")]
mod connection;
#[cfg(feature = "runtime")]
pub use connection::connect;
