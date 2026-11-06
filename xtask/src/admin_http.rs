const ADMIN_TIMEOUT_SECONDS: u64 = 5;
const SNAPSHOT_TIMEOUT_SECONDS: u64 = 300;
const MAX_RESPONSE_BYTES: usize = 131072;
const MAX_RESPONSE_READ_BYTES: u64 = 131073;

use std::net::SocketAddr;

pub struct Client {
    pub address: SocketAddr,
    pub token: String,
}
#[derive(Clone, Copy)]
pub enum Operation {
    Quiesce,
    Resume,
    Snapshot,
    Ready,
    Status,
    Calls,
    LabIncoming,
}
pub struct Response {
    pub status: u16,
    pub body: Option<serde_json::Value>,
}
impl Client {
    pub fn request(&self, operation: Operation) -> anyhow::Result<Response> {
        use std::io::{Read, Write};
        anyhow::ensure!(
            self.address.ip().is_loopback(),
            "administration connects only to loopback"
        );
        anyhow::ensure!(
            !self.token.is_empty()
                && self
                    .token
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-._".contains(&byte)),
            "invalid administration token"
        );
        let (method, path, body) = match operation {
            Operation::Quiesce => (
                "POST",
                "/api/admin/maintenance",
                Some(serde_json::json!({"enabled":true})),
            ),
            Operation::Resume => (
                "POST",
                "/api/admin/maintenance",
                Some(serde_json::json!({"enabled":false})),
            ),
            Operation::Snapshot => ("POST", "/api/admin/snapshot", None),
            Operation::Ready => ("GET", "/api/admin/ready", None),
            Operation::Status => ("GET", "/api/status", None),
            Operation::Calls => ("GET", "/api/calls", None),
            Operation::LabIncoming => (
                "POST",
                "/api/lab/incoming",
                Some(serde_json::json!({"scenario":"spam"})),
            ),
        };
        let body = body.map(|body| body.to_string()).unwrap_or_default();
        let mut socket = std::net::TcpStream::connect_timeout(
            &self.address,
            std::time::Duration::from_secs(ADMIN_TIMEOUT_SECONDS),
        )?;
        socket.set_read_timeout(Some(std::time::Duration::from_secs(
            if matches!(operation, Operation::Snapshot) {
                SNAPSHOT_TIMEOUT_SECONDS
            } else {
                ADMIN_TIMEOUT_SECONDS
            },
        )))?;
        socket.set_write_timeout(Some(std::time::Duration::from_secs(ADMIN_TIMEOUT_SECONDS)))?;
        write!(
            socket,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.address,
            self.token,
            body.len()
        )?;
        let mut bytes = vec![];
        socket
            .take(MAX_RESPONSE_READ_BYTES)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= MAX_RESPONSE_BYTES,
            "administration response exceeded its bound"
        );
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut parsed = httparse::Response::new(&mut headers);
        let offset = match parsed.parse(&bytes)? {
            httparse::Status::Complete(offset) => offset,
            httparse::Status::Partial => anyhow::bail!("incomplete administration response"),
        };
        anyhow::ensure!(
            !parsed
                .headers
                .iter()
                .any(|header| header.name.eq_ignore_ascii_case("transfer-encoding")),
            "administration requires a fixed response body"
        );
        let status = parsed
            .code
            .ok_or_else(|| anyhow::anyhow!("missing HTTP status"))?;
        let body = if bytes.len() == offset {
            None
        } else {
            Some(serde_json::from_slice(&bytes[offset..])?)
        };
        Ok(Response { status, body })
    }
}
