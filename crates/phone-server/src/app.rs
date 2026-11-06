const MAX_REQUEST_BYTES: usize = 16384;
const MAX_NUMBER_RULES: usize = 1000;
const MAX_RETENTION_DAYS: u32 = 365;
const MIN_RECORDING_BYTES: u64 = 1048576;
const EVENT_POLL_SECONDS: u64 = 10;
const EVENT_KEEPALIVE_SECONDS: u64 = 15;
const MAX_SIP_MESSAGE_BYTES: usize = 65536;
const P256_PUBLIC_KEY_BYTES: usize = 65;
const PUSH_AUTH_BYTES: usize = 16;
const DEV_LOCALHOST_ORIGIN: &str = "http://localhost:5173";
const DEV_LOOPBACK_ORIGIN: &str = "http://127.0.0.1:5173";

use crate::{
    config::{Config, Mode, valid_number},
    controller::{Control, Failure},
    gateway::Ari,
    model::{Call, Command, Event as CallEvent, Originate, PushSubscription, Settings},
    secrets::Secrets,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State, WebSocketUpgrade, ws::Message},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, net::SocketAddr, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tower_http::services::{ServeDir, ServeFile};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Clone)]
struct App {
    config: Config,
    control: Control,
    ari: Ari,
    secrets: Secrets,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    code: String,
    message: String,
}
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}
impl ApiError {
    fn invalid() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_input",
            message: "The request is invalid.",
        }
    }
    fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "origin_denied",
            message: "The request origin is not allowed.",
        }
    }
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "An allowed tailnet identity or API token is required.",
        }
    }
    fn missing() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "The requested resource does not exist.",
        }
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(_: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "operation_failed",
            message: "The operation failed.",
        }
    }
}
impl From<Failure> for ApiError {
    fn from(error: Failure) -> Self {
        match error {
            Failure::Busy => Self {
                status: StatusCode::CONFLICT,
                code: "line_busy",
                message: "The external line is busy.",
            },
            Failure::NotFound => Self::missing(),
            Failure::InvalidTransition => Self {
                status: StatusCode::CONFLICT,
                code: "invalid_transition",
                message: "The call does not permit this operation.",
            },
            Failure::InvalidInput => Self::invalid(),
            Failure::Unavailable => Self {
                status: StatusCode::SERVICE_UNAVAILABLE,
                code: "gateway_unavailable",
                message: "The telephone gateway is unavailable.",
            },
            Failure::Internal(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "operation_failed",
                message: "The operation failed.",
            },
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                code: self.code.into(),
                message: self.message.into(),
            }),
        )
            .into_response()
    }
}

#[derive(Serialize, ToSchema)]
pub struct Status {
    instance_id: Uuid,
    mode: String,
    gateway_ready: bool,
    ai_configured: bool,
    vapid_public_key: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct SipSession {
    domain: String,
    username: String,
    password: String,
}
#[derive(Serialize, ToSchema)]
pub struct Snapshot {
    calls: Vec<Call>,
    cursor: i64,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LabIncoming {
    scenario: String,
}
#[derive(Serialize, ToSchema)]
pub struct LabResult {
    channel_id: String,
}

fn require_admin(app: &App, headers: &HeaderMap) -> Result<(), ApiError> {
    let supplied = headers
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !app
        .secrets
        .api_token
        .as_deref()
        .zip(supplied)
        .is_some_and(|(expected, supplied)| {
            bool::from(expected.as_bytes().ct_eq(supplied.as_bytes()))
        })
    {
        return Err(ApiError::unauthorized());
    }
    Ok(())
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct Maintenance {
    enabled: bool,
}
#[derive(Serialize, ToSchema)]
struct SnapshotResult {
    id: Uuid,
}
#[utoipa::path(post,path="/api/admin/maintenance",request_body=Maintenance,responses((status=204),(status=409,body=ErrorBody)))]
async fn maintenance(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(input): Json<Maintenance>,
) -> Result<StatusCode, ApiError> {
    require_admin(&app, &headers)?;
    app.control.maintenance(input.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post,path="/api/admin/snapshot",responses((status=201,body=SnapshotResult),(status=409,body=ErrorBody)))]
async fn snapshot(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<(StatusCode, Json<SnapshotResult>), ApiError> {
    require_admin(&app, &headers)?;
    app.control.maintenance(true).await?;
    let path = crate::snapshot::create(&app.control.store, &app.config.data_dir).await?;
    let id = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| Uuid::parse_str(name).ok())
        .ok_or_else(ApiError::invalid)?;
    Ok((StatusCode::CREATED, Json(SnapshotResult { id })))
}
#[utoipa::path(get,path="/api/admin/ready",responses((status=204),(status=503,body=ErrorBody)))]
async fn candidate_ready(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    require_admin(&app, &headers)?;
    if !app.control.initialized() {
        return Err(Failure::Unavailable.into());
    }
    let application: serde_json::Value = app
        .ari
        .read(&["applications", app.ari.application()], &[])
        .await
        .map_err(|_| Failure::Unavailable)?;
    if application["name"] != app.ari.application() || !app.control.initialized() {
        return Err(Failure::Unavailable.into());
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn router(config: Config, control: Control, ari: Ari, secrets: Secrets) -> Router {
    let app = Arc::new(App {
        config,
        control,
        ari,
        secrets,
    });
    let api = Router::new()
        .route("/status", get(status))
        .route("/calls", get(calls).post(originate))
        .route("/calls/{id}", get(call).delete(delete_call))
        .route("/calls/{id}/commands", post(command))
        .route("/calls/{id}/recording", get(recording))
        .route("/settings", get(settings).put(save_settings))
        .route("/events", get(events))
        .route("/session", get(session))
        .route("/push-subscriptions", post(subscribe))
        .route("/lab/incoming", post(lab_incoming))
        .route("/openapi.json", get(|| async { Json(ApiDoc::openapi()) }))
        .route("/admin/maintenance", post(maintenance))
        .route("/admin/snapshot", post(snapshot))
        .route("/admin/ready", get(candidate_ready))
        .fallback(|| async { ApiError::missing() })
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(app.clone(), authorize));
    Router::new()
        .nest("/api", api)
        .route(
            "/sip",
            get(sip).layer(middleware::from_fn_with_state(app.clone(), authorize)),
        )
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({ "status": "running" })) }),
        )
        .fallback_service(
            ServeDir::new("web/dist").not_found_service(ServeFile::new("web/dist/index.html")),
        )
        .with_state(app)
}

async fn authorize(
    State(app): State<Arc<App>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let loopback = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| peer.0.ip().is_loopback());
    if !loopback {
        return Err(ApiError::unauthorized());
    }
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let token = app
        .secrets
        .api_token
        .as_deref()
        .zip(bearer)
        .is_some_and(|(expected, actual)| bool::from(expected.as_bytes().ct_eq(actual.as_bytes())));
    let identity = request
        .headers()
        .get("tailscale-user-login")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            app.config
                .allowed_users
                .iter()
                .any(|allowed| allowed == value)
        });
    if app.config.mode == Mode::Production && !token && !identity {
        return Err(ApiError::unauthorized());
    }
    if let Some(origin) = request.headers().get("origin") {
        let origin = origin.to_str().map_err(|_| ApiError::forbidden())?;
        let allowed = origin == app.config.public_origin
            || (app.config.mode == Mode::Lab
                && matches!(origin, DEV_LOCALHOST_ORIGIN | DEV_LOOPBACK_ORIGIN));
        if !allowed {
            return Err(ApiError::forbidden());
        }
    } else if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) && !token
    {
        return Err(ApiError::forbidden());
    }
    Ok(next.run(request).await)
}

#[utoipa::path(get, path="/api/status", responses((status=200, body=Status)))]
async fn status(State(app): State<Arc<App>>) -> Json<Status> {
    Json(service_status(&app))
}
fn service_status(app: &App) -> Status {
    Status {
        instance_id: app.config.instance_id,
        mode: match app.config.mode {
            Mode::Lab => "lab",
            Mode::Production => "production",
        }
        .into(),
        gateway_ready: app.control.ready(),
        ai_configured: app.config.ai.provider == "mock" || app.secrets.openai_key.is_some(),
        vapid_public_key: crate::notifications::public_key(app.secrets.vapid_pem.as_deref()),
    }
}
#[utoipa::path(get, path="/api/calls", responses((status=200, body=[Call])))]
async fn calls(State(app): State<Arc<App>>) -> Result<Json<Vec<Call>>, ApiError> {
    Ok(Json(app.control.store.calls().await?))
}
#[utoipa::path(get, path="/api/calls/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Call), (status=404, body=ErrorBody)))]
async fn call(State(app): State<Arc<App>>, Path(id): Path<Uuid>) -> Result<Json<Call>, ApiError> {
    app.control
        .store
        .get(id)
        .await?
        .map(Json)
        .ok_or_else(ApiError::missing)
}
#[utoipa::path(delete, path="/api/calls/{id}", params(("id"=Uuid, Path)), responses((status=204), (status=409, body=ErrorBody)))]
async fn delete_call(
    State(app): State<Arc<App>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let _recording_guard = app.control.store.recording_gate.read().await;
    app.control.deletable(id).await?;
    let Some(call) = app.control.store.get(id).await? else {
        return Ok(StatusCode::NO_CONTENT);
    };
    if call.ended_at.is_none() {
        return Err(Failure::InvalidTransition.into());
    }
    if let Some(event) = app
        .control
        .store
        .delete_finished(id, crate::controller::now())
        .await?
    {
        tracing::info!(
            operation = "delete_history",
            code = "committed",
            event_id = event.id
        );
    }
    for extension in ["wav", "partial"] {
        crate::maintenance::remove_if_exists(
            &app.config
                .data_dir
                .join("recordings")
                .join(format!("{id}.{extension}")),
        )
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post, path="/api/calls", request_body=Originate, responses((status=201, body=Call), (status=409, body=ErrorBody), (status=503, body=ErrorBody)))]
async fn originate(
    State(app): State<Arc<App>>,
    Json(input): Json<Originate>,
) -> Result<(StatusCode, Json<Call>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(app.control.originate(input.number).await?),
    ))
}
#[utoipa::path(post, path="/api/calls/{id}/commands", params(("id"=Uuid, Path)), request_body=Command, responses((status=200, body=Call), (status=409, body=ErrorBody)))]
async fn command(
    State(app): State<Arc<App>>,
    Path(id): Path<Uuid>,
    Json(command): Json<Command>,
) -> Result<Json<Call>, ApiError> {
    Ok(Json(app.control.command(id, command).await?))
}
#[utoipa::path(get, path="/api/settings", responses((status=200, body=Settings)))]
async fn settings(State(app): State<Arc<App>>) -> Result<Json<Settings>, ApiError> {
    Ok(Json(app.control.store.settings().await?))
}
#[utoipa::path(put, path="/api/settings", request_body=Settings, responses((status=200, body=Settings)))]
async fn save_settings(
    State(app): State<Arc<App>>,
    Json(settings): Json<Settings>,
) -> Result<Json<Settings>, ApiError> {
    if settings.rules.len() > MAX_NUMBER_RULES
        || settings
            .retention_days
            .is_some_and(|days| !(1..=MAX_RETENTION_DAYS).contains(&days))
        || settings
            .recording_limit_bytes
            .is_some_and(|bytes| bytes < MIN_RECORDING_BYTES)
        || settings.webhook_url.as_ref().is_some_and(|value| {
            url::Url::parse(value).map_or(true, |url| {
                !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
            }) || app.secrets.webhook_secret.is_none()
        })
        || settings
            .rules
            .keys()
            .any(|number| !valid_number(number) && number != "anonymous")
    {
        return Err(ApiError::invalid());
    }
    app.control.store.set_settings(&settings).await?;
    Ok(Json(settings))
}
#[utoipa::path(get, path="/api/session", responses((status=200, body=SipSession)))]
async fn session(State(app): State<Arc<App>>) -> Json<SipSession> {
    Json(SipSession {
        domain: app.config.sip.domain.clone(),
        username: app.config.sip.username.clone(),
        password: app.secrets.sip_password.clone(),
    })
}

#[utoipa::path(get, path="/api/events", responses((status=200, description="Snapshot, live service status, and durable call events over SSE")))]
async fn events(
    State(app): State<Arc<App>>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let mut updates = app.control.events.subscribe();
    let mut readiness = app.control.readiness();
    let stream = async_stream::stream! {
        let mut cursor = match app.control.store.snapshot().await {
            Ok((calls, cursor)) => {
                let snapshot = Snapshot { calls, cursor };
                if let Ok(event) = Event::default().event("snapshot").json_data(snapshot) { yield Ok(event); }
                cursor
            }
            Err(_) => { yield Ok(Event::default().event("unavailable").data("storage_unavailable")); return; }
        };
        let _initial_readiness = *readiness.borrow_and_update();
        if let Ok(event) = Event::default().event("status").json_data(service_status(&app)) { yield Ok(event); }
        loop {
            let status_changed = tokio::select! {
                result = updates.recv() => {
                    if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) { return; }
                    false
                }
                result = readiness.changed() => {
                    if result.is_err() { return; }
                    true
                }
                _ = tokio::time::sleep(Duration::from_secs(EVENT_POLL_SECONDS)) => false,
            };
            if status_changed
                && let Ok(event) = Event::default().event("status").json_data(service_status(&app)) {
                yield Ok(event);
            }
            loop {
                let batch = match app.control.store.events_after(cursor).await {
                    Ok(batch) => batch,
                    Err(_) => { yield Ok(Event::default().event("unavailable").data("storage_unavailable")); return; }
                };
                if batch.is_empty() { break; }
                for update in batch {
                    cursor = update.id;
                    if let Ok(event) = Event::default().event("call").id(cursor.to_string()).json_data(update) { yield Ok(event); }
                }
            }
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(EVENT_KEEPALIVE_SECONDS)))
}

#[utoipa::path(get, path="/api/calls/{id}/recording", params(("id"=Uuid, Path)), responses((status=200, content_type="audio/wav"), (status=206, content_type="audio/wav"), (status=404, body=ErrorBody)))]
async fn recording(
    State(app): State<Arc<App>>,
    Path(id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let call = app
        .control
        .store
        .get(id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if !matches!(call.recording, crate::model::Recording::Available { .. }) {
        return Err(ApiError::missing());
    }
    use tower::ServiceExt as _;
    let path = app
        .config
        .data_dir
        .join("recordings")
        .join(format!("{id}.wav"));
    let mut response = match ServeFile::new(path).oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response.into_response())
}

async fn sip(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    socket: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    if !headers
        .get("sec-websocket-protocol")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|part| part.trim() == "sip"))
    {
        return Err(ApiError::invalid());
    }
    let mut upstream = app.ari.connect_sip().await.map_err(ApiError::from)?;
    Ok(socket.protocols(["sip"]).max_message_size(MAX_SIP_MESSAGE_BYTES).on_upgrade(move |socket| async move {
        let (mut browser_out, mut browser_in) = socket.split();
        loop {
            tokio::select! {
                frame = browser_in.next() => match frame {
                    Some(Ok(Message::Text(text))) => { if upstream.send(tokio_tungstenite::tungstenite::Message::Text(text.to_string().into())).await.is_err() { break; } }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(Message::Ping(data))) => { if browser_out.send(Message::Pong(data)).await.is_err() { break; } }
                    Some(Ok(_)) => {}
                },
                frame = upstream.next() => match frame {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => { if browser_out.send(Message::Text(text.to_string().into())).await.is_err() { break; } }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            }
        }
        if (upstream.close(None).await).is_err() {
            tracing::debug!(operation="app", code="websocket_close_failed");
        }
        if (browser_out.close().await).is_err() {
            tracing::debug!(operation="app", code="websocket_close_failed");
        }
    }))
}

#[utoipa::path(post, path="/api/push-subscriptions", request_body=PushSubscription, responses((status=201)))]
async fn subscribe(
    State(app): State<Arc<App>>,
    Json(subscription): Json<PushSubscription>,
) -> Result<StatusCode, ApiError> {
    use base64::Engine as _;
    let url = url::Url::parse(&subscription.endpoint).map_err(|_| ApiError::invalid())?;
    let encoding = &base64::engine::general_purpose::URL_SAFE_NO_PAD;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || encoding
            .decode(&subscription.keys.p256dh)
            .map_or(true, |value| value.len() != P256_PUBLIC_KEY_BYTES)
        || encoding
            .decode(&subscription.keys.auth)
            .map_or(true, |value| value.len() != PUSH_AUTH_BYTES)
    {
        return Err(ApiError::invalid());
    }
    app.control.store.subscribe(&subscription).await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(post, path="/api/lab/incoming", request_body=LabIncoming, responses((status=202, body=LabResult)))]
async fn lab_incoming(
    State(app): State<Arc<App>>,
    Json(input): Json<LabIncoming>,
) -> Result<(StatusCode, Json<LabResult>), ApiError> {
    if app.config.mode != Mode::Lab {
        return Err(ApiError::missing());
    }
    let caller = match input.scenario.as_str() {
        "spam" => "999001",
        "suspected_spam" => "999002",
        "human" => "999003",
        "message" => "999004",
        "silent" => "999005",
        _ => return Err(ApiError::invalid()),
    };
    let id = format!("hp-lab-{}", Uuid::new_v4());
    let endpoint = format!("Local/{}@home-lab", input.scenario);
    app.ari
        .command(
            reqwest::Method::POST,
            &["channels"],
            &[
                ("endpoint", &endpoint),
                ("channelId", &id),
                ("app", app.ari.application()),
                ("appArgs", "inbound"),
                ("callerId", caller),
            ],
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(LabResult { channel_id: id })))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        status,
        calls,
        call,
        delete_call,
        originate,
        command,
        settings,
        save_settings,
        session,
        events,
        recording,
        subscribe,
        lab_incoming,
        maintenance,
        snapshot,
        candidate_ready
    ),
    components(schemas(
        Call,
        crate::model::Phase,
        crate::model::Recording,
        crate::model::Transcript,
        crate::model::Rule,
        Settings,
        Command,
        Originate,
        CallEvent,
        ErrorBody,
        Status,
        SipSession,
        Snapshot,
        PushSubscription,
        crate::model::PushKeys,
        LabIncoming,
        LabResult,
        Maintenance,
        SnapshotResult
    ))
)]
pub struct ApiDoc;
