use crate::media::Purpose;
#[cfg(not(feature = "plugin-ai"))]
use crate::{config::Ai, gateway::Ari, media::Report};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct Handle {
    pub generation: Uuid,
    pub channel: String,
    pub commands: mpsc::Sender<String>,
    pub unplayed: watch::Receiver<Vec<String>>,
    pub stop: CancellationToken,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

pub struct Start {
    pub call_id: Uuid,
    pub generation: Uuid,
    pub channel: String,
    pub bridge: String,
    pub caller: String,
    pub purpose: Purpose,
}

#[cfg(feature = "plugin-ai")]
mod runtime;
#[cfg(feature = "plugin-ai")]
pub use runtime::start;

#[cfg(not(feature = "plugin-ai"))]
pub async fn start(
    _ari: &Ari,
    _ai: &Ai,
    _key: Option<String>,
    _start: Start,
    _reports: mpsc::Sender<Report>,
) -> anyhow::Result<Handle> {
    anyhow::bail!("AI plugin is not installed")
}
