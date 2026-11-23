pub async fn wait() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = terminate.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
            }
            Err(_) => {
                if tokio::signal::ctrl_c().await.is_err() {
                    tracing::warn!(operation = "shutdown_signal", code = "ctrl_c_failed");
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        if tokio::signal::ctrl_c().await.is_err() {
            tracing::warn!(operation = "shutdown_signal", code = "ctrl_c_failed");
        }
    }
}
