use anyhow::{Context, ensure};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;

pub(crate) async fn json<T: DeserializeOwned>(
    response: reqwest::Response,
    limit: usize,
) -> anyhow::Result<T> {
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= limit as u64),
        "HTTP response exceeds its byte limit"
    );
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            phone_core::bytes::bounded_add(bytes.len(), chunk.len(), limit).is_some(),
            "HTTP response exceeds its byte limit"
        );
        bytes.try_reserve(chunk.len())?;
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("invalid HTTP JSON response")
}
