mod files;
mod output;
mod plans;

use output::LoginIssue;
pub use output::Output;

use super::{http::validate_public_origin, model::*};
use anyhow::{Context as _, ensure};
use clap::{Subcommand, ValueEnum};
use futures_util::StreamExt as _;
use reqwest::{Method, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{IsTerminal as _, Read as _},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};
use url::Url;

const MAX_INPUT: u64 = 2 * 1024 * 1024;
const MAX_RESPONSE: usize = 8 * 1024 * 1024;
const COOKIE: &str = "homephone_console";

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "HT813の管理者アカウントでログインします。")]
    Login {
        #[arg(long)]
        origin: Url,
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value = "admin")]
        username: String,
        #[arg(long, conflicts_with = "password_stdin")]
        password_file: Option<PathBuf>,
        #[arg(long)]
        password_stdin: bool,
    },
    #[command(about = "本体の状態と設定を取得します。")]
    Get {
        #[arg(long)]
        session: PathBuf,
        #[arg(long, default_value = "status")]
        page: Page,
        #[arg(long)]
        parameter: Vec<String>,
    },
    #[command(about = "各項目の入力範囲・初期値・推奨値を確認します。")]
    Docs {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        parameter: Option<String>,
    },
    #[command(about = "本体へ送信する変更案を作成して検証します。")]
    Plan {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        page: Page,
        #[arg(long)]
        changes: PathBuf,
        #[arg(long, value_enum)]
        mode: Mode,
        #[arg(long)]
        output: PathBuf,
    },
    #[command(about = "確認済みの変更案を一度送信し、保存内容を確認します。")]
    Apply {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        plan: PathBuf,
    },
    #[command(about = "本体の現在値と送信済みの変更案を比較します。")]
    Reconcile {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        plan: PathBuf,
    },
    #[command(about = "ログインを終了し、保存したセッションを削除します。")]
    Logout {
        #[arg(long)]
        session: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Page {
    Status,
    Basic,
    Advanced,
    Fxs,
    Fxo,
}
impl From<Page> for PageId {
    fn from(value: Page) -> Self {
        match value {
            Page::Status => Self::Status,
            Page::Basic => Self::Basic,
            Page::Advanced => Self::Advanced,
            Page::Fxs => Self::Fxs,
            Page::Fxo => Self::Fxo,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Mode {
    Save,
    Apply,
}
impl From<Mode> for WriteMode {
    fn from(value: Mode) -> Self {
        match value {
            Mode::Save => Self::Save,
            Mode::Apply => Self::SaveAndApply,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Session {
    schema: u8,
    origin: Url,
    cookie: String,
    claims: PathBuf,
}
impl Session {
    fn read(path: &Path) -> anyhow::Result<Self> {
        let state: Self = private_json(path)?;
        ensure!(
            state.schema == 1 && valid_cookie(&state.cookie) && state.claims.is_absolute(),
            "invalid private CLI session"
        );
        validate_public_origin(&state.origin)?;
        Ok(state)
    }
}

struct Api {
    client: reqwest::Client,
    origin: Url,
    cookie: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("console refused the request (HTTP {status}, code {code})")]
struct ApiFailure {
    status: u16,
    code: String,
}
impl Api {
    fn new(origin: Url, cookie: Option<String>) -> anyhow::Result<Self> {
        validate_public_origin(&origin)?;
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(300))
                .build()?,
            origin,
            cookie,
        })
    }
    fn authenticated(state: &Session) -> anyhow::Result<Self> {
        Self::new(state.origin.clone(), Some(state.cookie.clone()))
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        input: Option<&impl Serialize>,
    ) -> anyhow::Result<reqwest::Response> {
        let url = self.origin.join(&format!("/api/console/{path}"))?;
        let mut request = self
            .client
            .request(method, url)
            .header(header::ORIGIN, self.origin.origin().ascii_serialization());
        if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, format!("{COOKIE}={cookie}"));
        }
        if let Some(input) = input {
            request = request.json(input);
        }
        request
            .send()
            .await
            .context("console request failed; a submitted write may still complete")
    }
    async fn get<T: DeserializeOwned>(&self, path: &str) -> anyhow::Result<T> {
        response(self.request(Method::GET, path, None::<&()>).await?).await
    }
    async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        value: &impl Serialize,
    ) -> anyhow::Result<T> {
        response(self.request(Method::POST, path, Some(value)).await?).await
    }
    async fn page(&self, page: PageId) -> anyhow::Result<PageSnapshot> {
        let snapshot: PageSnapshot = self.get(&format!("pages/{}", page.key())).await?;
        ensure!(
            snapshot.page == page && !snapshot.revision.is_empty(),
            "invalid console snapshot identity"
        );
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }
    async fn revoke(&self) -> anyhow::Result<()> {
        let result = self.request(Method::DELETE, "session", None::<&()>).await?;
        ensure!(
            result.status() == reqwest::StatusCode::NO_CONTENT,
            "session revocation was not confirmed"
        );
        Ok(())
    }
}

async fn response<T: DeserializeOwned>(response: reqwest::Response) -> anyhow::Result<T> {
    let status = response.status();
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_RESPONSE as u64),
        "console response exceeds its byte limit"
    );
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.context("console response was interrupted; reconcile any submitted write")?;
        ensure!(
            chunk.len() <= MAX_RESPONSE.saturating_sub(bytes.len()),
            "console response exceeds its byte limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        let problem = serde_json::from_slice::<Problem>(&bytes)
            .ok()
            .filter(|problem| {
                matches!(
                    problem.code.as_str(),
                    "authentication_required"
                        | "device_unavailable"
                        | "unsupported_form"
                        | "stale_revision"
                        | "invalid_change"
                        | "device_busy"
                        | "access_denied"
                        | "session_limit"
                )
            });
        return Err(ApiFailure {
            status: status.as_u16(),
            code: problem
                .map(|problem| problem.code)
                .unwrap_or_else(|| "invalid_response".into()),
        }
        .into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("console returned an invalid response"))
}

fn validate_snapshot(snapshot: &PageSnapshot) -> anyhow::Result<()> {
    let mut parameters = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    ensure!(snapshot.settings.len() <= 1024, "too many console controls");
    for setting in &snapshot.settings {
        let field = &setting.field;
        ensure!(
            parameters.insert(&field.parameter)
                && ids.insert(&field.id)
                && !field.id.is_empty()
                && !field.parameter.is_empty()
                && (field.kind != FieldKind::Password || field.secret)
                && (!field.secret || (field.values.is_empty() && field.choices.is_empty())),
            "console controls violate identity or secret redaction"
        );
    }
    Ok(())
}

fn valid_cookie(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn login_cookie(response: &reqwest::Response, secure: bool) -> anyhow::Result<String> {
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter(|value| value.starts_with(&format!("{COOKIE}=")))
        .collect::<Vec<_>>();
    ensure!(
        cookies.len() == 1,
        "console returned ambiguous session cookies"
    );
    let parts = cookies[0].split(';').map(str::trim).collect::<Vec<_>>();
    let token = parts[0]
        .strip_prefix(&format!("{COOKIE}="))
        .context("missing session cookie")?;
    ensure!(
        valid_cookie(token)
            && parts.contains(&"Path=/api/console")
            && parts.contains(&"HttpOnly")
            && parts.contains(&"SameSite=Strict")
            && (!secure || parts.contains(&"Secure"))
            && !parts
                .iter()
                .any(|part| part.to_ascii_lowercase().starts_with("domain=")),
        "console returned an unsupported session cookie"
    );
    Ok(token.to_owned())
}

fn private_json<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    serde_json::from_slice(&files::read(path, MAX_INPUT)?)
        .map_err(|_| anyhow::anyhow!("invalid private CLI document"))
}

fn login_secret(bytes: Vec<u8>) -> anyhow::Result<String> {
    if bytes.is_empty() || bytes == b"\n" || bytes == b"\r\n" {
        return Err(LoginIssue::EmptyPassword.into());
    }
    files::secret(bytes)
}

pub async fn run(command: Command, output: &Output) -> anyhow::Result<ExitCode> {
    match command {
        Command::Login {
            origin,
            session,
            username,
            password_file,
            password_stdin,
        } => {
            let session_path = files::new_path(&session)?;
            validate_public_origin(&origin)?;
            if username.is_empty() {
                return Err(LoginIssue::EmptyUsername.into());
            }
            let password = if let Some(path) = password_file {
                login_secret(files::read(&path, 16 * 1024)?)?
            } else if password_stdin {
                ensure!(
                    !std::io::stdin().is_terminal(),
                    "use the hidden prompt for interactive password entry"
                );
                let mut bytes = vec![];
                std::io::stdin()
                    .take(16 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= 16 * 1024,
                    "password input exceeds its byte limit"
                );
                login_secret(bytes)?
            } else {
                rpassword::prompt_password("HT813の管理者パスワード（入力は表示されません）: ")?
            };
            if password.is_empty() {
                return Err(LoginIssue::EmptyPassword.into());
            }
            if username.len() > 120 || password.len() > 120 {
                return Err(LoginIssue::TooLong.into());
            }
            output.activity("HT813で認証しています");
            let mut api = Api::new(origin.clone(), None)?;
            let result = api
                .request(
                    Method::POST,
                    "session",
                    Some(&Login {
                        username: username.clone(),
                        password,
                    }),
                )
                .await?;
            if !result.status().is_success() {
                return Err(LoginIssue::Denied(result.status().as_u16()).into());
            }
            let cookie = login_cookie(&result, origin.scheme() == "https")?;
            api.cookie = Some(cookie.clone());
            let retained: anyhow::Result<SessionInfo> = async {
                let info: SessionInfo = response(result).await?;
                ensure!(
                    info.authenticated && info.username.as_ref() == Some(&username),
                    "device authentication was not confirmed"
                );
                let state = Session {
                    schema: 1,
                    origin,
                    cookie,
                    claims: files::claims(&session_path)?,
                };
                files::write(&session_path, &serde_json::to_vec(&state)?)?;
                Ok(info)
            }
            .await;
            let info = match retained {
                Ok(info) => info,
                Err(error) => {
                    if let Err(cleanup) = api.revoke().await {
                        anyhow::bail!("{error:#}; session revocation also failed: {cleanup:#}");
                    }
                    return Err(error);
                }
            };
            output.session(&info, Some(&api.origin), &session_path)?;
        }
        Command::Get {
            session,
            page,
            parameter,
        } => {
            let state = Session::read(&session)?;
            output.activity("本体の設定を取得しています");
            let mut snapshot = Api::authenticated(&state)?.page(page.into()).await?;
            for requested in &parameter {
                ensure!(
                    snapshot
                        .settings
                        .iter()
                        .any(|setting| setting.field.parameter == *requested),
                    "requested parameter is absent from the page"
                );
            }
            if !parameter.is_empty() {
                snapshot
                    .settings
                    .retain(|setting| parameter.contains(&setting.field.parameter));
            }
            output.page(&snapshot)?;
        }
        Command::Docs { session, parameter } => {
            let state = Session::read(&session)?;
            output.activity("項目の説明を取得しています");
            let mut help: Vec<Help> = Api::authenticated(&state)?.get("documentation").await?;
            if let Some(parameter) = parameter {
                help.retain(|entry| {
                    entry.parameters.contains(&parameter) || entry.key == parameter
                });
                ensure!(!help.is_empty(), "requested parameter has no documentation");
            }
            output.docs(&help)?;
        }
        Command::Plan {
            session,
            page,
            changes,
            mode,
            output: path,
        } => {
            plans::create(
                Session::read(&session)?,
                page.into(),
                &changes,
                mode.into(),
                &path,
                output,
                &session,
            )
            .await?;
        }
        Command::Apply { session, plan } => {
            return plans::apply(Session::read(&session)?, &plan, output, &session).await;
        }
        Command::Reconcile { session, plan } => {
            return plans::reconcile(Session::read(&session)?, &plan, output).await;
        }
        Command::Logout { session } => {
            let state = Session::read(&session)?;
            output.activity("ログアウトしています");
            Api::authenticated(&state)?.revoke().await?;
            std::fs::remove_file(&session)?;
            output.session(
                &SessionInfo {
                    authenticated: false,
                    username: None,
                },
                None,
                &session,
            )?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
