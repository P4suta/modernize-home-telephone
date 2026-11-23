use std::{path::Path, time::Duration};

pub const LOOKUP_BOUND: Duration = Duration::from_millis(200);

pub async fn caller_label(data_dir: &Path, caller: &str) -> Option<String> {
    #[cfg(feature = "plugin-phonebook")]
    {
        let data_dir = data_dir.to_owned();
        let caller = caller.to_owned();
        let lookup =
            tokio::task::spawn_blocking(move || phone_plugin_phonebook::label(&data_dir, &caller));
        match tokio::time::timeout(LOOKUP_BOUND, lookup).await {
            Ok(Ok(Ok(label))) => label,
            Ok(Ok(Err(_)) | Err(_)) => {
                tracing::warn!(operation = "caller_directory", code = "lookup_failed");
                None
            }
            Err(_) => {
                tracing::warn!(operation = "caller_directory", code = "lookup_timeout");
                None
            }
        }
    }
    #[cfg(not(feature = "plugin-phonebook"))]
    {
        let _ = (data_dir, caller);
        None
    }
}
