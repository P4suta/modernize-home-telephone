use axum::http::HeaderMap;

pub(crate) fn authenticated(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all("tailscale-user-login").iter();
    let identity = values.next().and_then(|value| value.to_str().ok());
    values.next().is_none()
        && identity.is_some_and(|value| {
            !value.is_empty() && value.trim() == value && !value.chars().any(char::is_control)
        })
}
