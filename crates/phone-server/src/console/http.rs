use super::{
    documentation,
    model::*,
    native::{MAX_HTML_BYTES, NativeForm, ScriptResources, script_resource_urls},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::StreamExt;
use phone_core::console::{Phase, Revision, SubmissionGuard, WritePlan};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, Mutex as SyncMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Semaphore};
use tower_http::services::{ServeDir, ServeFile};
use url::Url;
use utoipa::OpenApi;
use uuid::Uuid;

const MAX_SESSIONS: usize = 8;
const IDLE_LIMIT: Duration = Duration::from_secs(24 * 60 * 60);
const COOKIE: &str = "homephone_console";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    pub public_origin: Url,
    pub device_origin: Url,
    pub assets: PathBuf,
}
impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.listen.ip().is_loopback(),
            "the console must listen on loopback"
        );
        validate_http_origin(&self.device_origin)?;
        validate_public_origin(&self.public_origin)
    }
}

fn validate_http_origin(url: &Url) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "configure an HTTP origin without credentials, path, query, or fragment"
    );
    Ok(())
}

pub fn validate_public_origin(url: &Url) -> anyhow::Result<()> {
    validate_http_origin(url)?;
    let local = url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
    anyhow::ensure!(
        local || url.scheme() == "https",
        "remote access requires HTTPS"
    );
    Ok(())
}

#[derive(OpenApi)]
#[openapi(components(schemas(
    PageId,
    FieldKind,
    Choice,
    Field,
    Information,
    Login,
    SessionInfo,
    WriteMode,
    WriteReview,
    Changes,
    Help,
    IntegerRange,
    ParameterReference,
    HelpTopic,
    Setting,
    PageSnapshot,
    WriteOutcome,
    WriteReceipt,
    Problem
)))]
pub struct ApiDoc;

struct App {
    config: Config,
    sessions: SyncMutex<HashMap<String, Arc<Session>>>,
    gate: Arc<Semaphore>,
    catalogue: Vec<Help>,
}
struct Session {
    device: Mutex<Device>,
    last_used: SyncMutex<Instant>,
    revoked: AtomicBool,
}
struct Device {
    view: Uuid,
    client: reqwest::Client,
    username: String,
    password: String,
    pages: BTreeMap<PageId, Cached>,
}
struct Cached {
    form: NativeForm,
    guard: SubmissionGuard,
}
struct ExpectedSetting {
    parameter: String,
    kind: FieldKind,
    secret: bool,
}
struct PreparedWrite {
    plan: WritePlan,
    endpoint: Url,
    values: Vec<(String, String)>,
    effective: BTreeMap<String, Vec<String>>,
    expected: BTreeMap<String, ExpectedSetting>,
}
struct Document {
    html: String,
    url: Url,
    resources: ScriptResources,
}
#[derive(Clone)]
struct Failure(StatusCode, &'static str, &'static str);
impl Failure {
    fn auth() -> Self {
        Self(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "ログインが必要です。\n編集中の値を残したまま再ログインできます。",
        )
    }
    fn native() -> Self {
        Self(
            StatusCode::BAD_GATEWAY,
            "device_unavailable",
            "機器との通信に失敗しました。\n取得済みの値と編集内容は残しています。",
        )
    }
    fn unsupported() -> Self {
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_form",
            "機器のフォームを安全に扱えることを確認できません。\n設定は送信していません。",
        )
    }
    fn stale() -> Self {
        Self(
            StatusCode::CONFLICT,
            "stale_revision",
            "機器の値が変わりました。\n再取得した値と編集内容を比較してから確認してください。",
        )
    }
    fn invalid() -> Self {
        Self(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_change",
            "入力値を確認してください。\n変更可能な項目、選択肢、入力範囲を確認してください。",
        )
    }
    fn busy() -> Self {
        Self(
            StatusCode::CONFLICT,
            "device_busy",
            "機器との通信が進行中です。\n完了後に再取得してください。",
        )
    }
}
impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(Problem {
                code: self.1.into(),
                message: self.2.into(),
            }),
        )
            .into_response()
    }
}

pub fn router(config: Config) -> anyhow::Result<Router> {
    config.validate()?;
    let assets = config.assets.clone();
    let state = Arc::new(App {
        config,
        sessions: SyncMutex::new(HashMap::new()),
        gate: Arc::new(Semaphore::new(1)),
        catalogue: documentation::catalogue(),
    });
    let api = Router::new()
        .route("/session", get(session_info).post(login).delete(logout))
        .route("/pages/{page}", get(read_page).post(write_page))
        .route("/pages/{page}/review", axum::routing::post(review_page))
        .route("/documentation", get(catalogue))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), access))
        .with_state(state);
    Ok(Router::new()
        .nest("/api/console", api)
        .fallback_service(
            ServeDir::new(&assets)
                .precompressed_gzip()
                .precompressed_br()
                .not_found_service(ServeFile::new(assets.join("404.html"))),
        )
        .layer(middleware::from_fn(response_headers)))
}

async fn response_headers(request: Request, next: Next) -> Response {
    let api = request.uri().path().starts_with("/api/");
    let immutable = request.uri().path().starts_with("/_app/immutable/");
    let icon = request.uri().path().starts_with("/icons/");
    let mut response = next.run(request).await;
    let cacheable = response.status().is_success();
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    if api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    } else if immutable && cacheable {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    } else if icon && cacheable {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=86400, must-revalidate"),
        );
    } else {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    response
}
async fn access(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let origin = app.config.public_origin.origin().ascii_serialization();
    let mutation = !matches!(*request.method(), Method::GET | Method::HEAD);
    let wrong_origin = mutation
        && headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(origin.as_str());
    let cross_site = headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| !matches!(value, "same-origin" | "none"));
    let remote_denied =
        app.config.public_origin.scheme() == "https" && !crate::tailnet::authenticated(headers);
    if wrong_origin || cross_site || remote_denied {
        return Failure(
            StatusCode::FORBIDDEN,
            "access_denied",
            "管理画面へのアクセスが許可されていません。",
        )
        .into_response();
    }
    next.run(request).await
}
fn session(app: &App, headers: &HeaderMap) -> Result<Arc<Session>, Failure> {
    let id = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .split(';')
                .find_map(|item| item.trim().strip_prefix(&format!("{COOKIE}=")))
        })
        .ok_or_else(Failure::auth)?;
    let mut sessions = app.sessions.lock().map_err(|_| Failure::native())?;
    sessions.retain(|_, session| {
        !session.revoked.load(Ordering::Acquire)
            && session
                .last_used
                .lock()
                .is_ok_and(|time| time.elapsed() < IDLE_LIMIT)
    });
    let found = sessions.get(id).cloned().ok_or_else(Failure::auth)?;
    *found.last_used.lock().map_err(|_| Failure::native())? = Instant::now();
    Ok(found)
}
async fn session_info(State(app): State<Arc<App>>, headers: HeaderMap) -> Json<SessionInfo> {
    match session(&app, &headers) {
        Ok(session) => {
            let device = session.device.lock().await;
            Json(SessionInfo {
                authenticated: true,
                username: Some(device.username.clone()),
            })
        }
        Err(_) => Json(SessionInfo {
            authenticated: false,
            username: None,
        }),
    }
}
fn cookie(app: &App, id: &str, clear: bool) -> Result<HeaderValue, Failure> {
    let secure = if app.config.public_origin.scheme() == "https" {
        "; Secure"
    } else {
        ""
    };
    format!(
        "{COOKIE}={id}; Path=/api/console; HttpOnly; SameSite=Strict{secure}{}",
        if clear { "; Max-Age=0" } else { "" }
    )
    .parse()
    .map_err(|_| Failure::native())
}
async fn login(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Response, Failure> {
    if input.username.is_empty()
        || input.username.len() > 120
        || input.password.is_empty()
        || input.password.len() > 120
    {
        return Err(Failure::invalid());
    }
    let _permit = app
        .gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| Failure::busy())?;
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(75))
        .build()
        .map_err(|_| Failure::native())?;
    let device = Device {
        view: Uuid::new_v4(),
        client,
        username: input.username,
        password: input.password,
        pages: BTreeMap::new(),
    };
    device.authenticate(&app.config).await?;
    let confirmed = device.get(&app.config, PageId::Status.path()).await?;
    if NativeForm::parse(&confirmed.html, &confirmed.url).is_ok_and(|form| form.is_login()) {
        return Err(Failure::auth());
    }
    if let Ok(previous) = session(&app, &headers) {
        previous.revoked.store(true, Ordering::Release);
    }
    let username = device.username.clone();
    let id = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    {
        let mut sessions = app.sessions.lock().map_err(|_| Failure::native())?;
        sessions.retain(|_, session| {
            !session.revoked.load(Ordering::Acquire)
                && session
                    .last_used
                    .lock()
                    .is_ok_and(|time| time.elapsed() < IDLE_LIMIT)
        });
        if sessions.len() >= MAX_SESSIONS {
            return Err(Failure(
                StatusCode::TOO_MANY_REQUESTS,
                "session_limit",
                "ログイン数が上限に達しています。\n使用していない画面からログアウトしてください。",
            ));
        }
        sessions.insert(
            id.clone(),
            Arc::new(Session {
                device: Mutex::new(device),
                last_used: SyncMutex::new(Instant::now()),
                revoked: AtomicBool::new(false),
            }),
        );
    }
    Ok((
        [(header::SET_COOKIE, cookie(&app, &id, false)?)],
        Json(SessionInfo {
            authenticated: true,
            username: Some(username),
        }),
    )
        .into_response())
}
async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Response, Failure> {
    if let Ok(session) = session(&app, &headers) {
        session.revoked.store(true, Ordering::Release);
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cookie(&app, "", true)?)],
    )
        .into_response())
}
async fn catalogue(State(app): State<Arc<App>>) -> Json<Vec<Help>> {
    Json(app.catalogue.clone())
}
async fn read_page(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(page): Path<PageId>,
) -> Result<Json<PageSnapshot>, Failure> {
    let session = session(&app, &headers)?;
    let _permit = app
        .gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| Failure::busy())?;
    let mut device = session.device.lock().await;
    if session.revoked.load(Ordering::Acquire) {
        return Err(Failure::auth());
    }
    let (document, recovered) = device.read(&app.config, page).await?;
    let snapshot = device.observe(page, document, &app.catalogue, recovered)?;
    Ok(Json(snapshot))
}
async fn review_page(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(page): Path<PageId>,
    Json(changes): Json<Changes>,
) -> Result<Json<WriteReview>, Failure> {
    let session = session(&app, &headers)?;
    let _permit = app
        .gate
        .clone()
        .try_acquire_owned()
        .map_err(|_| Failure::busy())?;
    let device = session.device.lock().await;
    if session.revoked.load(Ordering::Acquire) {
        return Err(Failure::auth());
    }
    let prepared = device.prepare(&app, page, &changes)?;
    Ok(Json(WriteReview {
        page,
        revision: changes.revision,
        mode: changes.mode,
        changes: prepared.effective.len(),
        secret_changes: prepared
            .expected
            .values()
            .filter(|setting| setting.secret)
            .count(),
    }))
}

async fn write_page(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path(page): Path<PageId>,
    Json(changes): Json<Changes>,
) -> Result<Json<WriteReceipt>, Failure> {
    let session = session(&app, &headers)?;
    let task = tokio::spawn(async move {
        let _permit = app
            .gate
            .clone()
            .try_acquire_owned()
            .map_err(|_| Failure::busy())?;
        let mut device = session.device.lock().await;
        if session.revoked.load(Ordering::Acquire) {
            return Err(Failure::auth());
        }
        device.write(&app, page, changes, &session.revoked).await
    });
    task.await.map_err(|_| Failure::native())?.map(Json)
}

impl Device {
    fn review_token(&self, page: PageId, revision: Revision) -> String {
        format!("{}:{}:{}", self.view.simple(), page.key(), revision.get())
    }

    fn reviewed_revision(&self, page: PageId, token: &str) -> Result<Revision, Failure> {
        let number = token.rsplit(':').next().ok_or_else(Failure::stale)?;
        let revision = number
            .parse::<u64>()
            .ok()
            .and_then(|number| Revision::new(number).ok())
            .ok_or_else(Failure::stale)?;
        if self.review_token(page, revision) != token {
            return Err(Failure::stale());
        }
        Ok(revision)
    }
    async fn get(&self, config: &Config, path: &str) -> Result<Document, Failure> {
        let mut url = config
            .device_origin
            .join(path)
            .map_err(|_| Failure::unsupported())?;
        for _ in 0..4 {
            let response = self
                .client
                .get(url.clone())
                .send()
                .await
                .map_err(|_| Failure::native())?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(Failure::unsupported)?;
                let next = url.join(location).map_err(|_| Failure::unsupported())?;
                if next.origin() != config.device_origin.origin()
                    || !next.path().starts_with("/cgi-bin/")
                    || !next.username().is_empty()
                    || next.password().is_some()
                    || next.fragment().is_some()
                {
                    return Err(Failure::unsupported());
                }
                url = next;
                continue;
            }
            if !response.status().is_success() {
                return Err(Failure::native());
            }
            let html = body(response).await?;
            let mut resources = ScriptResources::default();
            for resource in script_resource_urls(&html, &url) {
                let Ok(response) = self
                    .client
                    .get(resource.clone())
                    .timeout(Duration::from_secs(3))
                    .send()
                    .await
                else {
                    break;
                };
                if !response.status().is_success() {
                    break;
                }
                let Ok(source) = body(response).await else {
                    break;
                };
                resources.record(resource.path(), source.as_bytes());
            }
            return Ok(Document {
                html,
                url,
                resources,
            });
        }
        Err(Failure::unsupported())
    }
    async fn authenticate(&self, config: &Config) -> Result<(), Failure> {
        let document = self.get(config, PageId::Status.path()).await?;
        let form =
            NativeForm::parse(&document.html, &document.url).map_err(|_| Failure::unsupported())?;
        let values = form
            .login_values(&self.username, &self.password)
            .map_err(|_| Failure::invalid())?;
        let response = self
            .client
            .post(form.endpoint().clone())
            .form(&normalize(values))
            .send()
            .await
            .map_err(|_| Failure::native())?;
        if response.status().is_client_error() {
            return Err(Failure::auth());
        }
        if !(response.status().is_success() || response.status().is_redirection()) {
            return Err(Failure::native());
        }
        let _ = body(response).await?;
        Ok(())
    }
    async fn read(&self, config: &Config, page: PageId) -> Result<(Document, bool), Failure> {
        let document = self.get(config, page.path()).await?;
        if !NativeForm::parse(&document.html, &document.url).is_ok_and(|form| form.is_login()) {
            return Ok((document, false));
        }
        self.authenticate(config).await?;
        let document = self.get(config, page.path()).await?;
        if NativeForm::parse(&document.html, &document.url).is_ok_and(|form| form.is_login()) {
            return Err(Failure::auth());
        }
        Ok((document, true))
    }
    fn observe(
        &mut self,
        page: PageId,
        document: Document,
        catalogue: &[Help],
        recovered: bool,
    ) -> Result<PageSnapshot, Failure> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Failure::native())?
            .as_millis();
        if page == PageId::Status {
            return Ok(PageSnapshot {
                page,
                revision: self.review_token(page, Revision::new(1).map_err(|_| Failure::native())?),
                fetched_at_ms: u64::try_from(now).map_err(|_| Failure::native())?,
                settings: vec![],
                information: information(&document.html)?,
                can_save: false,
                can_apply: false,
                reconciliation_required: false,
                undocumented: 0,
                native_session_recovered: recovered,
            });
        }
        let form = NativeForm::parse_verified(&document.html, &document.url, &document.resources)
            .or_else(|_| NativeForm::read(&document.html, &document.url))
            .map_err(|_| Failure::unsupported())?;
        if form.is_login() {
            return Err(Failure::auth());
        }
        let guard = if let Some(previous) = self.pages.get(&page) {
            previous
                .guard
                .refreshed(!previous.form.same_settings(&form))
                .map_err(|_| Failure::stale())?
        } else {
            SubmissionGuard::new(Revision::new(1).map_err(|_| Failure::native())?)
        };
        let settings: Vec<_> = form
            .fields()
            .iter()
            .map(|field| {
                let help = documentation::find(catalogue, &form, &field.id, &field.label).cloned();
                let mut field = field.clone();
                if let Some(help) = &help {
                    field.label = help.title.clone();
                    field.read_only |= help.read_only;
                } else {
                    field.values.clear();
                    field.choices.clear();
                    field.secret = true;
                    field.read_only = true;
                }
                Setting { field, help }
            })
            .collect();
        let undocumented = settings
            .iter()
            .filter(|setting| setting.help.is_none())
            .count();
        let snapshot = PageSnapshot {
            page,
            revision: self.review_token(page, guard.revision()),
            fetched_at_ms: u64::try_from(now).map_err(|_| Failure::native())?,
            can_save: form.supports_mode(phone_core::console::Mode::Save),
            can_apply: form.supports_mode(phone_core::console::Mode::SaveAndApply),
            settings,
            information: vec![],
            reconciliation_required: guard.phase() != Phase::Ready,
            undocumented,
            native_session_recovered: recovered,
        };
        self.pages.insert(page, Cached { form, guard });
        Ok(snapshot)
    }
    fn prepare(
        &self,
        app: &App,
        page: PageId,
        changes: &Changes,
    ) -> Result<PreparedWrite, Failure> {
        let revision = self.reviewed_revision(page, &changes.revision)?;
        let cached = self.pages.get(&page).ok_or_else(Failure::stale)?;
        if cached.guard.revision() != revision {
            return Err(Failure::stale());
        }
        for (id, values) in &changes.values {
            let field = cached
                .form
                .fields()
                .iter()
                .find(|field| field.id == *id)
                .ok_or_else(Failure::invalid)?;
            if !documentation::find(&app.catalogue, &cached.form, id, &field.label)
                .is_some_and(|help| documentation::valid_values(help, values))
            {
                return Err(Failure::invalid());
            }
        }
        let effective: BTreeMap<_, _> = changes
            .values
            .iter()
            .filter(|(id, values)| {
                cached
                    .form
                    .fields()
                    .iter()
                    .any(|field| field.id == **id && (field.secret || field.values != **values))
            })
            .map(|(id, values)| (id.clone(), values.clone()))
            .collect();
        let plan = cached
            .guard
            .plan(revision, effective.len(), changes.mode.into())
            .map_err(|_| Failure::stale())?;
        let values = cached
            .form
            .settings_values(&effective, changes.mode.into())
            .map_err(|_| Failure::invalid())?;
        let endpoint = cached.form.endpoint().clone();
        let expected: BTreeMap<_, _> = cached
            .form
            .fields()
            .iter()
            .filter(|field| effective.contains_key(&field.id))
            .map(|field| {
                (
                    field.id.clone(),
                    ExpectedSetting {
                        parameter: field.parameter.clone(),
                        kind: field.kind,
                        secret: field.secret,
                    },
                )
            })
            .collect();
        Ok(PreparedWrite {
            plan,
            endpoint,
            values,
            effective,
            expected,
        })
    }

    async fn write(
        &mut self,
        app: &App,
        page: PageId,
        changes: Changes,
        revoked: &AtomicBool,
    ) -> Result<WriteReceipt, Failure> {
        let revision = self.reviewed_revision(page, &changes.revision)?;
        if self
            .pages
            .get(&page)
            .is_none_or(|cached| cached.guard.revision() != revision)
        {
            return Err(Failure::stale());
        }
        let (document, recovered) = self.read(&app.config, page).await?;
        let snapshot = self.observe(page, document, &app.catalogue, recovered)?;
        if snapshot.revision != changes.revision {
            return Err(Failure::stale());
        }
        let PreparedWrite {
            plan,
            endpoint,
            values,
            effective,
            expected,
        } = self.prepare(app, page, &changes)?;
        if revoked.load(Ordering::Acquire) {
            return Err(Failure::auth());
        }
        let secret_change = expected.values().any(|setting| setting.secret);
        self.pages.get_mut(&page).ok_or_else(Failure::stale)?.guard =
            plan.guard().submitted().map_err(|_| Failure::stale())?;
        let operation_id = Uuid::new_v4();
        tracing::info!(operation="console_write", %operation_id, ?page, mode=?changes.mode,phase="submitting");
        let accepted = match self
            .client
            .post(endpoint)
            .form(&normalize(values))
            .send()
            .await
        {
            Ok(response) => {
                let success = response.status().is_success();
                body(response).await.is_ok() && success
            }
            Err(_) => false,
        };
        let observed = match self.read(&app.config, page).await {
            Ok((document, recovered)) => {
                self.observe(page, document, &app.catalogue, recovered).ok()
            }
            Err(_) => None,
        };
        let matched = observed.as_ref().is_some_and(|snapshot| {
            effective.iter().all(|(id, values)| {
                snapshot.settings.iter().any(|setting| {
                    setting.field.id == *id
                        && expected.get(id).is_some_and(|expected| {
                            setting.field.parameter == expected.parameter
                                && setting.field.kind == expected.kind
                                && setting.field.secret == expected.secret
                                && (expected.secret || setting.field.values == *values)
                        })
                })
            })
        });
        let outcome = if accepted && matched && secret_change {
            WriteOutcome::VerificationRequired
        } else if accepted && matched {
            WriteOutcome::Confirmed
        } else {
            WriteOutcome::Uncertain
        };
        tracing::info!(operation="console_write",%operation_id,?page,?outcome,phase="readback");
        Ok(WriteReceipt {
            outcome,
            snapshot: observed,
        })
    }
}

async fn body(response: reqwest::Response) -> Result<String, Failure> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_HTML_BYTES as u64)
    {
        return Err(Failure::unsupported());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| Failure::native())?;
        if phone_core::bytes::bounded_add(bytes.len(), chunk.len(), MAX_HTML_BYTES).is_none() {
            return Err(Failure::unsupported());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| Failure::unsupported())
}
fn normalize(values: Vec<(String, String)>) -> Vec<(String, String)> {
    fn lines(value: String) -> String {
        value
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', "\r\n")
    }
    values
        .into_iter()
        .map(|(key, value)| (lines(key), lines(value)))
        .collect()
}
fn information(html: &str) -> Result<Vec<Information>, Failure> {
    let parsed = scraper::Html::parse_document(html);
    let rows = scraper::Selector::parse("table tr").map_err(|_| Failure::unsupported())?;
    let excluded = scraper::Selector::parse("input,select,textarea,script,table")
        .map_err(|_| Failure::unsupported())?;
    fn text(node: scraper::ElementRef<'_>) -> String {
        node.text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn value(text: &str) -> String {
        match text
            .to_ascii_lowercase()
            .replace([' ', '_', '-'], "")
            .as_str()
        {
            "onhook" => "受話器が置かれています",
            "offhook" => "受話器が上がっています",
            "idle" => "待機中",
            "busy" | "inuse" => "使用中",
            "registered" => "登録済み",
            "notregistered" => "未登録",
            "yes" => "はい",
            "no" => "いいえ",
            _ => text,
        }
        .into()
    }
    fn label(text: &str) -> &str {
        match text {
            "Product Model" => "製品名",
            "Hardware Version" => "ハードウェアの版",
            "Part Number" => "部品番号",
            "Serial Number" => "シリアル番号",
            "MAC Address" => "MACアドレス",
            "Software Version" => "ファームウェア",
            "Program" | "Prog" => "プログラムの版",
            "Boot" | "Bootloader" => "起動プログラムの版",
            "Core" => "Coreの版",
            "Base" => "Baseの版",
            "System Up Time" | "System Uptime" | "Uptime" => "連続稼働時間",
            "System Time" => "機器の時刻",
            "WAN IP Address" => "WANのIPアドレス",
            "LAN IP Address" => "LANのIPアドレス",
            "IP Address" => "IPアドレス",
            "Subnet Mask" => "サブネットマスク",
            "Gateway" | "Default Gateway" => "ゲートウェイ",
            "DNS Server" => "DNSサーバー",
            "NAT" => "NATの状態",
            _ => text,
        }
    }
    fn append(values: &mut Vec<Information>, label: String, value: String) -> Result<(), Failure> {
        if label.is_empty() || value.is_empty() {
            return Ok(());
        }
        if label.len() > 256 || value.len() > 4096 || values.len() >= 512 {
            return Err(Failure::unsupported());
        }
        values.push(Information { label, value });
        Ok(())
    }
    let mut values = vec![];
    let mut headings = HashMap::new();
    for row in parsed.select(&rows) {
        let cells: Vec<_> = row
            .children()
            .filter_map(scraper::ElementRef::wrap)
            .filter(|node| matches!(node.value().name(), "td" | "th"))
            .collect();
        if cells
            .iter()
            .any(|cell| cell.select(&excluded).next().is_some())
        {
            continue;
        }
        let table = row
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .find(|node| node.value().name() == "table")
            .map(|node| node.id())
            .ok_or_else(Failure::unsupported)?;
        fn key(value: &str) -> String {
            value
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .flat_map(char::to_lowercase)
                .collect()
        }
        let raw_columns: Vec<_> = cells.iter().map(|cell| text(*cell)).collect();
        let heading = cells.len() >= 2
            && raw_columns
                .iter()
                .any(|name| matches!(key(name).as_str(), "port" | "portnumber"));
        let port = raw_columns.iter().find_map(|cell| match cell.as_str() {
            "FXS" | "FXS Port" => Some("FXS"),
            "FXO" | "FXO Port" => Some("FXO"),
            _ => None,
        });
        let mut columns = vec![];
        if heading || port.is_some() {
            for (cell, content) in cells.iter().zip(raw_columns) {
                let span = cell
                    .value()
                    .attr("colspan")
                    .unwrap_or("1")
                    .parse::<usize>()
                    .map_err(|_| Failure::unsupported())?;
                if span == 0 || span > 64 || columns.len() + span > 64 {
                    return Err(Failure::unsupported());
                }
                columns.extend(std::iter::repeat_n(content, span));
            }
        }
        if heading {
            headings.insert(table, columns);
            continue;
        }
        if let (Some(port), Some(headers)) = (port, headings.get(&table))
            && columns.len() == headers.len()
        {
            let mut seen = std::collections::BTreeSet::new();
            for (heading, content) in headers.iter().zip(&columns) {
                let field = match key(heading).as_str() {
                    "" | "port" | "portnumber" => continue,
                    "hook" | "hookstate" | "hookstatus" if port == "FXS" => "受話器",
                    "hook" | "hookstate" | "hookstatus" => "回線",
                    "registration"
                    | "registrationstatus"
                    | "sipregistration"
                    | "sipregistrationstatus" => "SIP登録",
                    "dnd" | "donotdisturb" => "着信拒否",
                    _ => heading,
                };
                if seen.insert((field, content)) {
                    append(&mut values, format!("{port}: {field}"), value(content))?;
                }
            }
        } else if cells.len() == 2 {
            let raw_label = text(cells[0]);
            let name = raw_label.trim_end_matches([':', '-', ' ']).trim();
            let raw_value = text(cells[1]);
            if matches!(name, "FXS" | "FXO")
                && let Some((hook, registration)) = raw_value.split_once('/')
            {
                append(
                    &mut values,
                    format!("{name}: {}", if name == "FXS" { "受話器" } else { "回線" }),
                    value(hook.trim()),
                )?;
                append(
                    &mut values,
                    format!("{name}: SIP登録"),
                    value(registration.trim()),
                )?;
            } else {
                append(&mut values, label(name).into(), value(&raw_value))?;
            }
        }
    }
    if values.is_empty() {
        return Err(Failure::unsupported());
    }
    Ok(values)
}

#[cfg(test)]
#[test]
fn status_rows_separate_registration_from_hook_state_and_ignore_table_wrappers() {
    let html = "<table><tr><td><table><tr><td>Product Model</td><td>HT813</td></tr><tr><td>Port</td><td>Hook</td><td>Registration</td><td>DND</td></tr><tr><td>FXS</td><td>On Hook</td><td>Not Registered</td><td>No</td></tr><tr><td>FXO</td><td>Idle</td><td>Registered</td><td>No</td></tr></table></td><td></td></tr></table>";
    let values = information(html).map_err(|_| "unsupported status").unwrap();
    assert_eq!(values.len(), 7);
    assert!(
        values
            .iter()
            .any(|row| row.label == "製品名" && row.value == "HT813")
    );
    assert!(
        values
            .iter()
            .any(|row| row.label == "FXS: SIP登録" && row.value == "未登録")
    );
    assert!(
        values
            .iter()
            .any(|row| row.label == "FXS: 受話器" && row.value == "受話器が置かれています")
    );
    assert!(
        values
            .iter()
            .any(|row| row.label == "FXO: SIP登録" && row.value == "登録済み")
    );
    assert!(
        values
            .iter()
            .any(|row| row.label == "FXO: 回線" && row.value == "待機中")
    );
}

#[cfg(test)]
#[test]
fn status_columns_follow_headers_across_spacers_and_separate_port_tables() {
    let html = "<table><tr><td colspan='100'>Layout heading</td></tr></table><table><tr><td>Port</td><td>Hook</td><td></td><td>Registration</td></tr><tr><td>FXS</td><td>On Hook</td><td></td><td>Not Registered</td></tr></table><table><tr><td>Port</td><td>DND</td></tr><tr><td>FXS</td><td>No</td></tr></table><table><tr><td>Port</td><td>DND</td><td>Registration</td><td>Hook</td></tr><tr><td>FXO</td><td>Yes</td><td>Registered</td><td>Idle</td></tr></table>";
    let rows = information(html).map_err(|_| "unsupported status").unwrap();
    for (label, value) in [
        ("FXS: 受話器", "受話器が置かれています"),
        ("FXS: SIP登録", "未登録"),
        ("FXS: 着信拒否", "いいえ"),
        ("FXO: 回線", "待機中"),
        ("FXO: SIP登録", "登録済み"),
        ("FXO: 着信拒否", "はい"),
    ] {
        assert!(
            rows.iter()
                .any(|row| row.label == label && row.value == value),
            "missing {label} = {value}"
        );
    }
    assert_eq!(rows.len(), 6);
}
