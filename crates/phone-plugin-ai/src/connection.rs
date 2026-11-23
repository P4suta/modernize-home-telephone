use anyhow::Context;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig},
};
use tokio_util::sync::CancellationToken;

pub async fn connect(
    model: &str,
    key: &str,
    stop: &CancellationToken,
) -> anyhow::Result<Option<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>> {
    let mut url = url::Url::parse("wss://api.openai.com/v1/realtime")?;
    url.query_pairs_mut().append_pair("model", model);
    let mut request = url.as_str().into_client_request()?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {key}"))?;
    authorization.set_sensitive(true);
    request.headers_mut().insert("authorization", authorization);
    let limits = WebSocketConfig::default()
        .max_message_size(Some(524288))
        .max_frame_size(Some(524288));
    tokio::select! {
        _ = stop.cancelled() => Ok(None),
        connected = tokio::time::timeout(std::time::Duration::from_secs(5), tokio_tungstenite::connect_async_with_config(request, Some(limits), false)) => {
            let (socket, _) = connected.context("AI connection timed out")??;
            Ok(Some(socket))
        }
    }
}
