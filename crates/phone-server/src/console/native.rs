use super::model::{Choice, Field, FieldKind};
use base64::Engine as _;
use phone_core::console::Mode;
use scraper::{ElementRef, Html, Selector};
use std::collections::BTreeMap;
use url::Url;

mod profile;
pub use profile::Resources as ScriptResources;
pub use profile::resource_urls as script_resource_urls;

pub const MAX_HTML_BYTES: usize = 1_048_576;
const MAX_CONTROLS: usize = 1024;
const MAX_VALUE_BYTES: usize = 16_384;
const MAX_CHOICES: usize = 256;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum NativeError {
    #[error("the device form is unsupported")]
    UnsupportedForm,
    #[error("the form endpoint is outside the configured device")]
    InvalidEndpoint,
    #[error("the form token is missing or duplicated")]
    InvalidToken,
    #[error("the credentials do not fit the native form")]
    InvalidCredentials,
    #[error("the field change is invalid")]
    InvalidChange,
    #[error("the device response exceeds the form limit")]
    TooLarge,
}

#[derive(Clone, Eq, PartialEq)]
struct Control {
    name: String,
    field: Option<usize>,
    kind: Option<FieldKind>,
    values: Vec<String>,
    choices: Vec<Choice>,
    disabled: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Purpose {
    Login,
    Settings,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Operation {
    Login,
    Save,
    Apply,
}

#[derive(Clone, Eq, PartialEq)]
struct Submitter {
    position: usize,
    name: String,
    value: String,
    operation: Operation,
}

pub struct NativeForm {
    endpoint: Url,
    fields: Vec<Field>,
    controls: Vec<Control>,
    submitters: Vec<Submitter>,
    purpose: Purpose,
    plain_settings: bool,
    profile: Option<profile::Plan>,
}

fn selector(value: &str) -> Result<Selector, NativeError> {
    Selector::parse(value).map_err(|_| NativeError::UnsupportedForm)
}

fn text(node: ElementRef<'_>) -> String {
    node.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn label(node: ElementRef<'_>, document: &Html) -> Result<String, NativeError> {
    if let Some(id) = node.value().attr("id")
        && let Some(found) = document
            .select(&selector("label")?)
            .find(|candidate| candidate.value().attr("for") == Some(id))
    {
        return Ok(text(found));
    }
    if let Some(row) = node
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|parent| parent.value().name() == "tr")
        && let Some(cell) = row
            .children()
            .filter_map(ElementRef::wrap)
            .find(|cell| matches!(cell.value().name(), "td" | "th"))
    {
        let value = text(cell);
        if !value.is_empty() {
            return Ok(value.trim_end_matches(':').trim().to_owned());
        }
    }
    Ok(node
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|parent| parent.value().name() == "label")
        .map(text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| node.value().attr("name").unwrap_or("").to_owned()))
}

fn japanese_label(value: &str) -> String {
    match value {
        "Primary SIP Server" | "SIP Server" => "SIPサーバー",
        "Account Active" => "アカウントを使用",
        "Preferred Vocoder" => "優先コーデック",
        "Dial Plan" => "ダイヤルプラン",
        "Call Waiting" => "割込通話",
        "Authenticate Password" => "認証パスワード",
        "Authenticate ID" => "認証ID",
        "SIP User ID" => "SIPユーザーID",
        "Local SIP Port" => "ローカルSIPポート",
        "Local RTP Port" => "ローカルRTPポート",
        "SIP Transport" => "SIPトランスポート",
        "Number of Rings" => "応答までの呼び出し回数",
        "PSTN Ring Thru FXS" => "電話機（FXS）も鳴らす",
        "PSTN Ring Thru Delay (sec)" => "電話機を鳴らすまでの遅延（秒）",
        "PSTN Ring Timeout (sec)" => "呼び出しのタイムアウト（秒）",
        "Enable Current Disconnect" => "回線電流の断を検出",
        "Current Disconnect Threshold (ms)" => "電流断の判定時間（ミリ秒）",
        "Enable PSTN Disconnect Tone Detection" => "切断音を検出",
        "Caller ID Scheme" => "発信者番号の通知方式",
        "Time Zone" => "タイムゾーン",
        "Firmware Server Path" => "ファームウェアサーバー",
        "Config Server Path" => "設定ファイルサーバー",
        "Syslog Server" => "Syslogサーバー",
        "Syslog Level" => "ログレベル",
        "New Admin Password" => "新しい管理者パスワード",
        "Username" => "ユーザー名",
        "Password" => "パスワード",
        value => value,
    }
    .to_owned()
}

fn sensitive_parameter(name: &str, label: &str, kind: FieldKind) -> bool {
    if matches!(
        name,
        "P2" | "P34"
            | "P734"
            | "P83"
            | "P1359"
            | "P1361"
            | "P279"
            | "P281"
            | "P8472"
            | "P4505"
            | "P4512"
            | "P8221"
            | "P21902"
            | "P21900"
            | "P21908"
            | "P21909"
            | "P21914"
            | "P21915"
            | "P28110"
            | "P28124"
            | "P28126"
            | "P196"
            | "P794"
            | "P795"
            | "P28113"
            | "P8606"
    ) {
        return true;
    }
    if !matches!(kind, FieldKind::Text | FieldKind::TextArea) {
        return false;
    }
    let normalized = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "password",
        "privatekey",
        "sharedsecret",
        "authenticationkey",
        "privacykey",
        "community",
        "ddnshash",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn choice_label(value: &str) -> String {
    match value.trim() {
        "Yes" => "はい",
        "Enabled" | "Enable" => "有効",
        "No" => "いいえ",
        "Disabled" | "Disable" => "無効",
        value => value,
    }
    .to_owned()
}

fn choice_text(node: ElementRef<'_>, document: &Html) -> Result<Option<String>, NativeError> {
    if let Some(id) = node.value().attr("id")
        && let Some(found) = document
            .select(&selector("label")?)
            .find(|label| label.value().attr("for") == Some(id))
    {
        return Ok(Some(text(found)));
    }
    if let Some(label) = node
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|node| node.value().name() == "label")
    {
        return Ok(Some(text(label)));
    }
    let mut parts = Vec::new();
    for sibling in node.next_siblings() {
        if let Some(element) = ElementRef::wrap(sibling) {
            if matches!(
                element.value().name(),
                "input" | "select" | "textarea" | "button" | "br"
            ) {
                break;
            }
            parts.push(text(element));
        } else if let Some(text) = sibling.value().as_text() {
            parts.push(text.to_string());
        }
    }
    let label = parts
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok((!label.is_empty()).then_some(label))
}

fn operation(value: &str) -> Option<Operation> {
    match value.trim().to_ascii_lowercase().as_str() {
        "login" | "log in" => Some(Operation::Login),
        "update" | "save" => Some(Operation::Save),
        "apply" | "save and apply" => Some(Operation::Apply),
        _ => None,
    }
}

fn disabled(node: ElementRef<'_>) -> bool {
    node.value().attr("disabled").is_some()
        || node.ancestors().filter_map(ElementRef::wrap).any(|parent| {
            if matches!(parent.value().name(), "select" | "optgroup") {
                return parent.value().attr("disabled").is_some();
            }
            parent.value().name() == "fieldset"
                && parent.value().attr("disabled").is_some()
                && !parent
                    .children()
                    .filter_map(ElementRef::wrap)
                    .find(|child| child.value().name() == "legend")
                    .is_some_and(|legend| node.ancestors().any(|item| item.id() == legend.id()))
        })
}

impl NativeForm {
    pub fn read(html: &str, document: &Url) -> Result<Self, NativeError> {
        Self::parse_with(html, document, false)
    }
    pub fn parse(html: &str, document: &Url) -> Result<Self, NativeError> {
        Self::parse_with(html, document, true)
    }
    pub fn parse_verified(
        html: &str,
        document: &Url,
        resources: &ScriptResources,
    ) -> Result<Self, NativeError> {
        if html.len() > MAX_HTML_BYTES {
            return Err(NativeError::TooLarge);
        }
        let parsed = Html::parse_document(html);
        let plan = profile::Plan::inspect(&parsed, document, resources);
        Self::parse_with_plan(html, document, true, plan)
    }
    fn parse_with(html: &str, document: &Url, strict: bool) -> Result<Self, NativeError> {
        Self::parse_with_plan(html, document, strict, None)
    }
    fn parse_with_plan(
        html: &str,
        document: &Url,
        strict: bool,
        profile: Option<profile::Plan>,
    ) -> Result<Self, NativeError> {
        if html.len() > MAX_HTML_BYTES {
            return Err(NativeError::TooLarge);
        }
        let parsed = Html::parse_document(html);
        let controls_selector = selector("input, select, textarea, button")?;
        let forms = parsed
            .select(&selector("form")?)
            .filter(|form| {
                form.select(&controls_selector).any(|node| {
                    let kind =
                        node.value()
                            .attr("type")
                            .unwrap_or(if node.value().name() == "button" {
                                "submit"
                            } else {
                                "text"
                            });
                    (kind.eq_ignore_ascii_case("submit") || kind.eq_ignore_ascii_case("button"))
                        && operation(node.value().attr("value").unwrap_or(""))
                            .or_else(|| operation(&text(node)))
                            .is_some()
                })
            })
            .collect::<Vec<_>>();
        let [form] = forms.as_slice() else {
            return Err(NativeError::UnsupportedForm);
        };
        if strict
            && (!form
                .value()
                .attr("method")
                .is_some_and(|method| method.eq_ignore_ascii_case("post"))
                || form
                    .value()
                    .attr("enctype")
                    .is_some_and(|value| value != "application/x-www-form-urlencoded"))
        {
            return Err(NativeError::UnsupportedForm);
        }
        let endpoint = document
            .join(form.value().attr("action").unwrap_or(""))
            .map_err(|_| NativeError::InvalidEndpoint)?;
        if endpoint.origin() != document.origin()
            || !matches!(endpoint.scheme(), "http" | "https")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
            || !endpoint.path().starts_with("/cgi-bin/")
        {
            return Err(NativeError::InvalidEndpoint);
        }
        let fragments: Vec<_> = profile
            .iter()
            .flat_map(|plan| plan.fragments())
            .map(|(hash, markup)| (hash, Html::parse_fragment(markup)))
            .collect();
        let node_selector = selector("input, select, textarea, button, script")?;
        let nodes = form
            .select(&node_selector)
            .flat_map(|node| {
                if node.value().name() == "script" {
                    let hash = profile::digest(node.text().collect::<String>().as_bytes());
                    fragments
                        .iter()
                        .find(|(known, _)| *known == hash)
                        .map(|(_, fragment)| {
                            fragment.select(&controls_selector).collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    vec![node]
                }
            })
            .collect::<Vec<_>>();
        if nodes.len() > MAX_CONTROLS {
            return Err(NativeError::TooLarge);
        }
        let mut fields: Vec<Field> = vec![];
        let mut controls = vec![];
        let mut submitters = vec![];
        let mut radio_fields: BTreeMap<String, usize> = BTreeMap::new();
        let mut event_handlers = form.value().attrs().any(|(name, _)| name.starts_with("on"));
        for node in nodes {
            let element = node.value();
            event_handlers |= element.attrs().any(|(name, _)| name.starts_with("on"));
            let Some(name) = element.attr("name").filter(|name| !name.is_empty()) else {
                continue;
            };
            if name.len() > 128 || name.chars().any(char::is_control) {
                return Err(NativeError::UnsupportedForm);
            }
            let tag = element.name();
            let input_type = element
                .attr("type")
                .unwrap_or(if tag == "button" { "submit" } else { "text" })
                .to_ascii_lowercase();
            let value = element.attr("value").unwrap_or("").to_owned();
            let is_disabled = disabled(node);
            if input_type == "submit" {
                if let Some(op) = operation(&value).or_else(|| operation(&text(node)))
                    && !is_disabled
                {
                    if ["formaction", "formmethod", "formenctype"]
                        .iter()
                        .any(|name| element.attr(name).is_some())
                    {
                        return Err(NativeError::UnsupportedForm);
                    }
                    submitters.push(Submitter {
                        position: controls.len(),
                        name: name.to_owned(),
                        value,
                        operation: op,
                    });
                }
                continue;
            }
            if matches!(input_type.as_str(), "button" | "reset" | "file" | "image") {
                continue;
            }
            if input_type == "hidden" {
                controls.push(Control {
                    name: name.to_owned(),
                    field: None,
                    kind: None,
                    values: vec![value],
                    choices: vec![],
                    disabled: is_disabled,
                });
                continue;
            }
            let kind = match (tag, input_type.as_str()) {
                ("select", _) => FieldKind::Select,
                ("textarea", _) => FieldKind::TextArea,
                (_, "password") => FieldKind::Password,
                (_, "radio") => FieldKind::Radio,
                (_, "checkbox") => FieldKind::Checkbox,
                (_, "number") => FieldKind::Number,
                (_, "text" | "tel" | "search") => FieldKind::Text,
                _ => return Err(NativeError::UnsupportedForm),
            };
            let mut choices: Vec<Choice> = vec![];
            let values = match kind {
                FieldKind::Select => {
                    let mut selected = vec![];
                    let mut seen = std::collections::BTreeSet::new();
                    for option in node.select(&selector("option")?) {
                        let option_text = text(option);
                        let value = option
                            .value()
                            .attr("value")
                            .unwrap_or(&option_text)
                            .to_owned();
                        if choices.len() >= MAX_CHOICES {
                            return Err(NativeError::UnsupportedForm);
                        }
                        if !seen.insert(value.clone()) {
                            if strict
                                && (profile.is_none()
                                    || element.attr("multiple").is_some()
                                    || choices.iter().any(|choice| {
                                        choice.value == value && choice.disabled != disabled(option)
                                    }))
                            {
                                return Err(NativeError::UnsupportedForm);
                            }
                            if let Some(existing) =
                                choices.iter_mut().find(|choice| choice.value == value)
                            {
                                existing.label =
                                    format!("{} / {}", existing.label, choice_label(&option_text));
                                existing.disabled &= disabled(option);
                            }
                            if option.value().attr("selected").is_some()
                                && (element.attr("multiple").is_none()
                                    || !selected.contains(&value))
                            {
                                selected.push(value);
                            }
                            continue;
                        }
                        if option.value().attr("selected").is_some() {
                            selected.push(value.clone());
                        }
                        choices.push(Choice {
                            value,
                            label: choice_label(&option_text),
                            disabled: disabled(option),
                        });
                    }
                    if element.attr("multiple").is_none() {
                        if let Some(value) = selected.last().cloned() {
                            selected = vec![value];
                        } else if let Some(first) = choices.first() {
                            selected = vec![first.value.clone()];
                        }
                    }
                    selected
                }
                FieldKind::TextArea => vec![node.text().collect::<String>()],
                FieldKind::Radio | FieldKind::Checkbox => {
                    let value = if element.attr("value").is_some() {
                        value
                    } else {
                        "on".into()
                    };
                    let choice = choice_text(node, &parsed)?.unwrap_or_else(|| {
                        if kind == FieldKind::Checkbox {
                            "有効".into()
                        } else {
                            value.clone()
                        }
                    });
                    choices.push(Choice {
                        value: value.clone(),
                        label: choice_label(&choice),
                        disabled: is_disabled,
                    });
                    if element.attr("checked").is_some() {
                        vec![value]
                    } else {
                        vec![]
                    }
                }
                FieldKind::Text | FieldKind::Password | FieldKind::Number => {
                    vec![value.replace(['\r', '\n'], "")]
                }
            };
            if values.iter().any(|value| value.len() > MAX_VALUE_BYTES) {
                return Err(NativeError::TooLarge);
            }
            let existing = if kind == FieldKind::Radio {
                radio_fields.get(name).copied()
            } else {
                None
            };
            let index = if let Some(index) = existing {
                let field = &mut fields[index];
                if choices
                    .iter()
                    .any(|choice| field.choices.iter().any(|old| old.value == choice.value))
                {
                    return Err(NativeError::UnsupportedForm);
                }
                field.choices.extend(choices.clone());
                field.disabled &= is_disabled;
                field.required |= element.attr("required").is_some();
                index
            } else {
                let index = fields.len();
                let source_label = label(node, &parsed)?;
                let secret =
                    kind == FieldKind::Password || sensitive_parameter(name, &source_label, kind);
                fields.push(Field {
                    id: format!("field-{index}"),
                    parameter: name.to_owned(),
                    secret,
                    label: japanese_label(&source_label),
                    description: source_label,
                    kind,
                    values: if secret { vec![] } else { values.clone() },
                    choices: choices.clone(),
                    disabled: is_disabled,
                    read_only: !strict
                        || (element.attr("readonly").is_some()
                            && !(profile.is_some() && profile::editable_password(node))),
                    multiple: element.attr("multiple").is_some(),
                    required: element.attr("required").is_some(),
                    max_length: element
                        .attr("maxlength")
                        .and_then(|value| value.parse().ok()),
                    min: element.attr("min").and_then(|value| value.parse().ok()),
                    max: element.attr("max").and_then(|value| value.parse().ok()),
                });
                if kind == FieldKind::Radio {
                    radio_fields.insert(name.to_owned(), index);
                }
                index
            };
            controls.push(Control {
                name: name.to_owned(),
                field: Some(index),
                kind: Some(kind),
                values,
                choices,
                disabled: is_disabled,
            });
        }
        for (name, field) in radio_fields {
            let last = controls
                .iter()
                .rposition(|control| control.name == name && !control.values.is_empty());
            for (index, control) in controls.iter_mut().enumerate() {
                if control.name == name
                    && control.kind == Some(FieldKind::Radio)
                    && Some(index) != last
                {
                    control.values.clear();
                }
            }
            fields[field].values = last
                .map(|index| controls[index].values.clone())
                .unwrap_or_default();
        }
        for field in &mut fields {
            if field.secret {
                field.values.clear();
                if matches!(
                    field.kind,
                    FieldKind::Select | FieldKind::Radio | FieldKind::Checkbox
                ) {
                    field.choices.clear();
                    field.read_only = true;
                }
            }
        }
        let token_count = controls
            .iter()
            .filter(|control| matches!(control.name.as_str(), "csrf_token" | "session_token"))
            .count();
        if strict && token_count != 1 {
            return Err(NativeError::InvalidToken);
        }
        for name in ["csrf_token", "session_token", "gnkey"] {
            let tokens = controls
                .iter()
                .filter(|control| control.name == name)
                .collect::<Vec<_>>();
            if strict
                && (tokens.len() > 1
                    || tokens.iter().any(|control| {
                        control.field.is_some()
                            || control.disabled
                            || control.values.iter().all(String::is_empty)
                    }))
            {
                return Err(NativeError::InvalidToken);
            }
        }
        let purpose = if submitters
            .iter()
            .any(|submitter| submitter.operation == Operation::Login)
        {
            Purpose::Login
        } else {
            Purpose::Settings
        };
        let scripts = parsed
            .select(&selector("script")?)
            .map(|node| node.text().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        let external_scripts = parsed
            .select(&selector("script")?)
            .any(|node| node.value().attr("src").is_some());
        if purpose == Purpose::Login {
            let expected = "functioncheckForm(){varinput_pwd=document.getElementById('input-password');input_pwd.value=btoa(input_pwd.value);returntrue;}";
            let compact = scripts.split_whitespace().collect::<String>();
            let handler = form
                .value()
                .attr("onsubmit")
                .unwrap_or("")
                .split_whitespace()
                .collect::<String>();
            if !compact.contains(expected)
                || !matches!(handler.as_str(), "returncheckForm()" | "returncheckForm();")
                || fields.len() != 2
                || fields.iter().any(|field| field.disabled || field.read_only)
                || !controls.iter().any(|control| control.name == "gnkey")
                || controls
                    .iter()
                    .filter(|control| {
                        control.name == "username" && control.kind == Some(FieldKind::Text)
                    })
                    .count()
                    != 1
                || controls
                    .iter()
                    .filter(|control| {
                        control.name == "P2" && control.kind == Some(FieldKind::Password)
                    })
                    .count()
                    != 1
            {
                return Err(NativeError::UnsupportedForm);
            }
        }
        Ok(Self {
            endpoint,
            fields,
            controls,
            submitters,
            purpose,
            plain_settings: strict
                && scripts.trim().is_empty()
                && !event_handlers
                && !external_scripts,
            profile,
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn is_login(&self) -> bool {
        self.purpose == Purpose::Login
    }

    pub fn parameter(&self, id: &str) -> Option<&str> {
        let index = self.fields.iter().position(|field| field.id == id)?;
        self.controls
            .iter()
            .find(|control| control.field == Some(index))
            .map(|control| control.name.as_str())
    }

    fn submitter(&self, operation: Operation) -> Result<&Submitter, NativeError> {
        let mut matching = self
            .submitters
            .iter()
            .filter(|submitter| submitter.operation == operation);
        let Some(value) = matching.next() else {
            return Err(NativeError::UnsupportedForm);
        };
        if matching.next().is_some() {
            return Err(NativeError::UnsupportedForm);
        }
        Ok(value)
    }

    pub fn supports_mode(&self, mode: Mode) -> bool {
        self.purpose == Purpose::Settings
            && (self.plain_settings || self.profile.is_some())
            && self
                .submitter(match mode {
                    Mode::Save => Operation::Save,
                    Mode::SaveAndApply => Operation::Apply,
                })
                .is_ok()
    }

    pub fn login_values(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Vec<(String, String)>, NativeError> {
        if self.purpose != Purpose::Login || username.is_empty() || password.is_empty() {
            return Err(NativeError::InvalidCredentials);
        }
        for (name, value) in [("username", username), ("P2", password)] {
            let control = self
                .controls
                .iter()
                .find(|control| control.name == name)
                .ok_or(NativeError::UnsupportedForm)?;
            let field = control
                .field
                .and_then(|index| self.fields.get(index))
                .ok_or(NativeError::UnsupportedForm)?;
            if field
                .max_length
                .is_some_and(|max| value.encode_utf16().count() > max as usize)
                || value.len() > MAX_VALUE_BYTES
                || value.contains(['\r', '\n'])
            {
                return Err(NativeError::InvalidCredentials);
            }
        }
        let latin1 = password
            .chars()
            .map(|value| {
                u8::try_from(u32::from(value)).map_err(|_| NativeError::InvalidCredentials)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(latin1);
        let button = self.submitter(Operation::Login)?;
        let mut values = self.base_values(&BTreeMap::new(), button);
        for (name, value) in &mut values {
            match name.as_str() {
                "username" => *value = username.to_owned(),
                "P2" => *value = encoded.clone(),
                _ => {}
            }
        }
        Ok(values)
    }

    pub fn settings_values(
        &self,
        changes: &BTreeMap<String, Vec<String>>,
        mode: Mode,
    ) -> Result<Vec<(String, String)>, NativeError> {
        if !self.supports_mode(mode) {
            return Err(NativeError::UnsupportedForm);
        }
        for (index, field) in self.fields.iter().enumerate() {
            if !field.required || field.disabled || field.read_only {
                continue;
            }
            let source_values = self
                .controls
                .iter()
                .filter(|control| control.field == Some(index))
                .flat_map(|control| control.values.iter())
                .collect::<Vec<_>>();
            let empty = match changes.get(&field.id) {
                Some(values) => values.iter().all(String::is_empty),
                None => source_values.iter().all(|value| value.is_empty()),
            };
            if empty {
                return Err(NativeError::InvalidChange);
            }
        }
        for (id, values) in changes {
            let field = self
                .fields
                .iter()
                .find(|field| field.id == *id)
                .ok_or(NativeError::InvalidChange)?;
            if field.disabled
                || field.read_only
                || values.len() > MAX_CHOICES
                || (!field.multiple && values.len() > 1)
            {
                return Err(NativeError::InvalidChange);
            }
            let scalar = matches!(
                field.kind,
                FieldKind::Text | FieldKind::Password | FieldKind::TextArea | FieldKind::Number
            );
            if (scalar && values.len() != 1)
                || (field.required && values.iter().all(String::is_empty))
                || (field.secret && values.iter().any(String::is_empty))
                || values.iter().any(|value| {
                    value.len() > MAX_VALUE_BYTES
                        || (matches!(field.parameter.as_str(), "P2" | "P196" | "P28113")
                            && value.chars().any(|character| u32::from(character) > 255))
                        || field
                            .max_length
                            .is_some_and(|max| value.encode_utf16().count() > max as usize)
                        || (field.kind != FieldKind::TextArea && value.contains(['\r', '\n']))
                        || (!scalar
                            && !field
                                .choices
                                .iter()
                                .any(|choice| choice.value == *value && !choice.disabled))
                })
            {
                return Err(NativeError::InvalidChange);
            }
            if field.kind == FieldKind::Number {
                for value in values {
                    let number = value
                        .parse::<i32>()
                        .map_err(|_| NativeError::InvalidChange)?;
                    if field.min.is_some_and(|min| number < min)
                        || field.max.is_some_and(|max| number > max)
                    {
                        return Err(NativeError::InvalidChange);
                    }
                }
            }
        }
        let button = self.submitter(match mode {
            Mode::Save => Operation::Save,
            Mode::SaveAndApply => Operation::Apply,
        })?;
        let values = self.base_values(changes, button);
        if let Some(profile) = &self.profile {
            profile.validate(&values)?;
        }
        Ok(values)
    }

    fn base_values(
        &self,
        changes: &BTreeMap<String, Vec<String>>,
        button: &Submitter,
    ) -> Vec<(String, String)> {
        let mut values = vec![];
        for (index, control) in self.controls.iter().enumerate() {
            if index == button.position {
                values.push((button.name.clone(), button.value.clone()));
            }
            if control.disabled {
                continue;
            }
            let changed = control
                .field
                .and_then(|index| self.fields.get(index))
                .and_then(|field| changes.get(&field.id));
            let selected = changed.unwrap_or(&control.values);
            for value in selected {
                if matches!(control.kind, Some(FieldKind::Radio | FieldKind::Checkbox))
                    && !control.choices.iter().any(|choice| choice.value == *value)
                {
                    continue;
                }
                if control.kind == Some(FieldKind::Select)
                    && control
                        .choices
                        .iter()
                        .any(|choice| choice.value == *value && choice.disabled)
                {
                    continue;
                }
                values.push((control.name.clone(), value.clone()));
            }
        }
        if button.position == self.controls.len() {
            values.push((button.name.clone(), button.value.clone()));
        }
        values
    }

    pub fn same_settings(&self, other: &Self) -> bool {
        self.purpose == other.purpose
            && self.endpoint == other.endpoint
            && self.fields == other.fields
            && self.submitters == other.submitters
            && self.plain_settings == other.plain_settings
            && self.profile == other.profile
            && self.controls.len() == other.controls.len()
            && self
                .controls
                .iter()
                .zip(&other.controls)
                .all(|(left, right)| {
                    if matches!(left.name.as_str(), "csrf_token" | "session_token" | "gnkey") {
                        left.name == right.name
                            && left.kind == right.kind
                            && left.field == right.field
                            && left.disabled == right.disabled
                    } else {
                        left == right
                    }
                })
    }
}
