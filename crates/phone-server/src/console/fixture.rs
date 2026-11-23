use super::http::{self, Config};
use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;

const LOGIN: &str = r#"<form method="post" action="dologin" onsubmit="return checkForm()">
<input type="hidden" name="csrf_token" value="fixture-csrf">
<input type="hidden" name="gnkey" value="fixture-key">
<label>Username<input name="username" value="admin" maxlength="30"></label>
<label>Password<input name="P2" type="password" id="input-password" maxlength="30"></label>
<input type="submit" name="Login" value="Login"></form>
<script>function checkForm(){var input_pwd=document.getElementById('input-password');input_pwd.value=btoa(input_pwd.value);return true;}</script>"#;

#[derive(Default, Serialize)]
struct Device {
    generation: u64,
    logins: usize,
    writes: usize,
    reads: usize,
    token: u64,
    values: BTreeMap<String, String>,
    fail_after_write: bool,
    read_delay_ms: u64,
}
type Shared = Arc<Mutex<Device>>;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Reset,
    Expire,
    FailAfterWrite,
    Set { parameter: String, value: String },
    Delay { milliseconds: u64 },
}

pub async fn serve(listen: SocketAddr, assets: PathBuf) -> anyhow::Result<()> {
    anyhow::ensure!(
        listen.ip().is_loopback(),
        "fixtures require a loopback listener"
    );
    anyhow::ensure!(
        assets.join("index.html").is_file(),
        "build console assets before browser verification"
    );
    let state = Shared::default();
    let native = TcpListener::bind("127.0.0.1:0").await?;
    let native_origin = format!("http://{}", native.local_addr()?);
    let native_app = Router::new()
        .route("/cgi-bin/dologin", post(login))
        .route("/cgi-bin/update", post(write))
        .route("/cgi-bin/{page}", get(read))
        .with_state(state.clone());
    let fixture = tokio::spawn(async move { axum::serve(native, native_app).await });
    let listener = TcpListener::bind(listen).await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let app = http::router(Config {
        listen,
        public_origin: origin.parse()?,
        device_origin: native_origin.parse()?,
        assets,
    })?
    .merge(
        Router::new()
            .route("/__fixture", get(statistics).post(control))
            .with_state(state),
    );
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(crate::shutdown::wait())
        .await;
    fixture.abort();
    result?;
    Ok(())
}

async fn statistics(State(state): State<Shared>) -> Json<serde_json::Value> {
    let state = state.lock().unwrap();
    Json(serde_json::json!({"writes":state.writes,"reads":state.reads,"logins":state.logins}))
}
async fn control(State(state): State<Shared>, Json(command): Json<Control>) -> StatusCode {
    let mut state = state.lock().unwrap();
    match command {
        Control::Reset => {
            let generation = state.generation + 1;
            *state = Device {
                generation,
                ..Device::default()
            };
        }
        Control::Expire => state.generation += 1,
        Control::FailAfterWrite => state.fail_after_write = true,
        Control::Set { parameter, value } => {
            if !parameter.starts_with('P') || parameter.len() > 16 || value.len() > 128 {
                return StatusCode::BAD_REQUEST;
            }
            state.values.insert(parameter, value);
        }
        Control::Delay { milliseconds } => {
            if milliseconds > 2000 {
                return StatusCode::BAD_REQUEST;
            }
            state.read_delay_ms = milliseconds;
        }
    }
    StatusCode::NO_CONTENT
}
async fn login(
    State(state): State<Shared>,
    Form(values): Form<BTreeMap<String, String>>,
) -> impl IntoResponse {
    let mut state = state.lock().unwrap();
    state.logins += 1;
    if values.get("username").map(String::as_str) != Some("admin")
        || values.get("P2").map(String::as_str) != Some("Zml4dHVyZS1jb25zb2xl")
        || values.get("csrf_token").map(String::as_str) != Some("fixture-csrf")
        || values.get("gnkey").map(String::as_str) != Some("fixture-key")
    {
        return (StatusCode::UNAUTHORIZED, HeaderMap::new(), LOGIN.to_owned());
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        format!("native={}; Path=/; HttpOnly", state.generation)
            .parse()
            .unwrap(),
    );
    (StatusCode::OK, headers, "authenticated".to_owned())
}
fn authenticated(state: &Device, headers: &HeaderMap) -> bool {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        == Some(format!("native={}", state.generation).as_str())
}
async fn read(State(state): State<Shared>, Path(page): Path<String>, headers: HeaderMap) -> String {
    let delay = state.lock().unwrap().read_delay_ms;
    if delay > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    }
    let mut state = state.lock().unwrap();
    state.reads += 1;
    if !authenticated(&state, &headers) {
        return LOGIN.into();
    }
    if page == "index" {
        return "<table><tr><td>Product Model</td><td>HT813</td></tr><tr><td>Program</td><td>1.0.17.3</td></tr><tr><td>FXS</td><td>On Hook / Not Registered</td></tr><tr><td>FXO</td><td>Idle / Not Registered</td></tr></table>".into();
    }
    state.token += 1;
    let mut rows = String::new();
    fn value(state: &Device, name: &str, default: &str) -> String {
        escape(
            state
                .values
                .get(name)
                .map(String::as_str)
                .unwrap_or(default),
        )
    }
    fn text(rows: &mut String, state: &Device, name: &str, label: &str, default: &str) {
        rows.push_str(&format!(
            "<tr><td>{label}:</td><td><input name='{name}' value='{}' maxlength='128'></td></tr>",
            value(state, name, default)
        ));
    }
    fn number(
        rows: &mut String,
        state: &Device,
        name: &str,
        label: &str,
        default: &str,
        min: i32,
        max: i32,
    ) {
        rows.push_str(&format!("<tr><td>{label}:</td><td><input type='number' name='{name}' value='{}' min='{min}' max='{max}'></td></tr>",value(state,name,default)));
    }
    match page.as_str() {
        "config2" => {
            let timezone = value(&state, "P64", "UTC");
            rows.push_str(&format!("<tr><td>Time Zone:</td><td><select name='P64'><option value='UTC' {}>UTC</option><option value='JST' {}>日本標準時（UTC+09:00）</option></select></td></tr>",if timezone=="UTC"{"selected"}else{""},if timezone=="JST"{"selected"}else{""}));
        }
        "config" => {
            text(&mut rows, &state, "P30", "NTP Server", "pool.ntp.org");
            text(&mut rows, &state, "P76", "STUN Server", "");
        }
        "config_a1" | "config_a2" => {
            let fxs = page == "config_a1";
            text(
                &mut rows,
                &state,
                if fxs { "P47" } else { "P747" },
                "Primary SIP Server",
                "sip.example.invalid",
            );
            text(
                &mut rows,
                &state,
                if fxs { "P35" } else { "P735" },
                "SIP User ID",
                "100",
            );
            let rtp = if fxs { "P39" } else { "P739" };
            number(
                &mut rows,
                &state,
                rtp,
                "Local RTP Port",
                if fxs { "5004" } else { "5012" },
                1024,
                65535,
            );
            let password = if fxs { "P34" } else { "P734" };
            rows.push_str(&format!("<tr><td>Authenticate Password:</td><td><input type='password' name='{password}' value='{}' maxlength='30'></td></tr>",value(&state,password,"fixture-existing")));
            let codec = if fxs { "P57" } else { "P757" };
            let current = value(&state, codec, "0");
            rows.push_str(&format!("<tr><td>Preferred Vocoder:</td><td><select name='{codec}'><option value='0' {}>PCMU</option><option value='8' {}>PCMA</option></select></td></tr>",if current=="0"{"selected"}else{""},if current=="8"{"selected"}else{""}));
            if !fxs {
                number(&mut rows, &state, "P798", "Number of Rings", "4", 1, 50);
                number(
                    &mut rows,
                    &state,
                    "P843",
                    "PSTN Ring Thru Delay (sec)",
                    "4",
                    1,
                    10,
                );
                number(
                    &mut rows,
                    &state,
                    "P3630",
                    "PSTN Ring Timeout (sec)",
                    "6",
                    2,
                    10,
                );
            }
        }
        _ => return LOGIN.into(),
    }
    format!(
        "<form method='post' action='update'><input type='hidden' name='csrf_token' value='token-{}'><input type='hidden' name='gnkey' value='fixture-key'><input type='hidden' name='future_flag' value='preserve'><table>{rows}</table><input type='submit' name='update' value='Update'><input type='submit' name='apply' value='Apply'></form>",
        state.token
    )
}
async fn write(
    State(state): State<Shared>,
    headers: HeaderMap,
    Form(values): Form<BTreeMap<String, String>>,
) -> StatusCode {
    let mut state = state.lock().unwrap();
    if !authenticated(&state, &headers)
        || values.get("csrf_token") != Some(&format!("token-{}", state.token))
        || values.get("gnkey").map(String::as_str) != Some("fixture-key")
        || values.get("future_flag").map(String::as_str) != Some("preserve")
        || !(values.contains_key("update") ^ values.contains_key("apply"))
    {
        return StatusCode::BAD_REQUEST;
    }
    state.writes += 1;
    for (name, value) in values {
        if name.starts_with('P') {
            state.values.insert(name, value);
        }
    }
    if std::mem::take(&mut state.fail_after_write) {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
