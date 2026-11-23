use super::ApiFailure;
use crate::console::model::*;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::{
    cell::RefCell,
    io::{IsTerminal as _, Write as _},
    path::Path,
    time::Duration,
};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub(super) enum LoginIssue {
    #[error("device password is empty")]
    EmptyPassword,
    #[error("device username is empty")]
    EmptyUsername,
    #[error("device credentials exceed the supported input limits")]
    TooLong,
    #[error("device authentication was refused (HTTP {0})")]
    Denied(u16),
}

pub struct Output {
    human: bool,
    emphasis: bool,
    progress_enabled: bool,
    progress: RefCell<Option<ProgressBar>>,
}
impl Output {
    pub fn new(json: bool, human: bool, no_progress: bool) -> Self {
        let terminal = std::io::stdout().is_terminal();
        let dumb = std::env::var_os("TERM").is_some_and(|value| value == "dumb");
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        let human = !json && (human || terminal);
        Self {
            human,
            emphasis: terminal && !dumb && !no_color,
            progress_enabled: human && std::io::stderr().is_terminal() && !dumb && !no_progress,
            progress: RefCell::new(None),
        }
    }
    pub fn is_human(&self) -> bool {
        self.human
    }
    pub(super) fn activity(&self, message: &'static str) {
        self.finish();
        if self.progress_enabled {
            let bar = ProgressBar::new_spinner();
            bar.set_style(
                ProgressStyle::with_template("  {spinner} {msg}")
                    .expect("static progress template"),
            );
            bar.set_message(message);
            bar.tick();
            bar.enable_steady_tick(Duration::from_millis(120));
            self.progress.replace(Some(bar));
        }
    }
    fn finish(&self) {
        if let Some(bar) = self.progress.borrow_mut().take() {
            bar.finish_and_clear();
        }
    }
    pub(super) fn emit<T: Serialize + ?Sized>(
        &self,
        value: &T,
        draw: impl FnOnce(&mut Screen),
    ) -> anyhow::Result<()> {
        self.finish();
        let mut output = std::io::stdout().lock();
        if self.human {
            let mut screen = Screen::new(self.emphasis);
            draw(&mut screen);
            output.write_all(screen.0.as_bytes())?;
        } else {
            serde_json::to_writer_pretty(&mut output, value)?;
            writeln!(output)?;
        }
        output.flush()?;
        Ok(())
    }
    pub(super) fn session(
        &self,
        value: &SessionInfo,
        origin: Option<&Url>,
        path: &Path,
    ) -> anyhow::Result<()> {
        self.emit(value, |screen| {
            if value.authenticated {
                screen.heading("✓ Authentication successful!");
                screen.line(&format!(
                    "Hi {}!",
                    value.username.as_deref().unwrap_or("admin")
                ));
                if let Some(origin) = origin {
                    screen.field("接続先", origin.as_str());
                }
                screen.field("認証情報", "セッションを保存しました。");
                screen.field(
                    "次の操作",
                    &self.command(&["get", "--session", &path.to_string_lossy()]),
                );
            } else {
                screen.heading("✓ ログアウトしました");
                screen.line("認証セッションを終了しました。");
            }
        })
    }
    pub(super) fn page(&self, snapshot: &PageSnapshot) -> anyhow::Result<()> {
        self.emit(snapshot, |screen| {
            screen.heading(&format!("✓ {}を取得しました", snapshot.page.title()));
            for item in &snapshot.information {
                screen.field(&item.label, &item.value);
            }
            for setting in &snapshot.settings {
                let field = &setting.field;
                let label = setting
                    .help
                    .as_ref()
                    .map_or(field.label.as_str(), |help| help.title.as_str());
                let value = if field.secret {
                    "非公開".to_owned()
                } else if field.values.is_empty() {
                    "未設定".to_owned()
                } else {
                    field
                        .values
                        .iter()
                        .map(|value| {
                            field
                                .choices
                                .iter()
                                .find(|choice| choice.value == *value)
                                .map_or(value.as_str(), |choice| choice.label.as_str())
                        })
                        .collect::<Vec<_>>()
                        .join(" / ")
                };
                screen.field(&format!("{} · {}", field.parameter, label), &value);
            }
            if snapshot.reconciliation_required {
                screen.field(
                    "確認が必要です",
                    "前回の適用結果を確認してから変更案を作成してください。",
                );
            }
            if snapshot.native_session_recovered {
                screen.line("本体の認証を更新しました。");
            }
        })
    }
    pub(super) fn docs(&self, help: &[Help]) -> anyhow::Result<()> {
        self.emit(help, |screen| {
            screen.heading("項目の説明");
            if help.len() > 1 {
                screen.line(&format!(
                    "{}項目 · --parameter P番号で説明を表示します。",
                    help.len()
                ));
                for entry in help {
                    screen.line(&format!("{}  {}", entry.parameters.join(", "), entry.title));
                }
            } else if let Some(entry) = help.first() {
                screen.field(
                    &format!("{} · {}", entry.parameters.join(", "), entry.title),
                    &entry.summary,
                );
                for (label, value) in [
                    ("入力", &entry.input),
                    ("初期値", &entry.default_value),
                    ("推奨", &entry.recommendation),
                    ("変更の影響", &entry.impact),
                    ("補足", &entry.note),
                    ("出典", &entry.source),
                ] {
                    if !value.is_empty() {
                        screen.field(label, value);
                    }
                }
            }
        })
    }
    pub(super) fn command(&self, arguments: &[&str]) -> String {
        std::iter::once(
            std::env::args_os()
                .next()
                .unwrap_or_else(|| "ht813-console".into())
                .to_string_lossy()
                .into_owned(),
        )
        .chain(arguments.iter().map(|value| (*value).to_owned()))
        .map(|value| quote(&value))
        .collect::<Vec<_>>()
        .join(" ")
    }
    pub(super) fn diagnostic(&self, error: &anyhow::Error) -> anyhow::Result<()> {
        self.finish();
        let mut output = std::io::stderr().lock();
        if self.human {
            let mut screen = Screen::new(false);
            screen.line(&format!("! {}", explanation(error)));
            output.write_all(screen.0.as_bytes())?;
        } else {
            writeln!(output, "{error}; reconcile before creating another plan")?;
        }
        output.flush()?;
        Ok(())
    }
    pub fn report(&self, error: &anyhow::Error, login: bool) -> anyhow::Result<()> {
        self.finish();
        let mut screen = Screen::new(self.emphasis && std::io::stderr().is_terminal());
        screen.heading(if login {
            "! Authentication failed"
        } else {
            "! 操作を完了できませんでした"
        });
        screen.line(&explanation(error));
        screen.field("操作の説明", &self.command(&["--help"]));
        let mut output = std::io::stderr().lock();
        output.write_all(screen.0.as_bytes())?;
        output.flush()?;
        Ok(())
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.finish();
    }
}

pub(super) struct Screen(String, bool);
impl Screen {
    fn new(emphasis: bool) -> Self {
        Self(String::new(), emphasis)
    }
    fn strong(&self, value: &str) -> String {
        let value = safe(value);
        if self.1 {
            format!("\x1b[1m{value}\x1b[0m")
        } else {
            value
        }
    }
    pub(super) fn heading(&mut self, title: &str) {
        self.0.push_str(&format!("\n  {}\n\n", self.strong(title)));
    }
    pub(super) fn line(&mut self, value: &str) {
        for line in value.split('\n') {
            self.0.push_str(&format!("  {}\n", safe(line)));
        }
        self.0.push('\n');
    }
    pub(super) fn field(&mut self, label: &str, value: &str) {
        self.0.push_str(&format!("  {}\n", self.strong(label)));
        for line in value.split('\n') {
            self.0.push_str(&format!("  {}\n", safe(line)));
        }
        self.0.push('\n');
    }
}
fn safe(value: &str) -> String {
    let mut visible = String::new();
    for character in value.chars() {
        if character.is_control()
            || matches!(character, '\u{200e}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            visible.extend(character.escape_default());
        } else {
            visible.push(character);
        }
    }
    visible
}
fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-./:=@".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}
fn explanation(error: &anyhow::Error) -> String {
    if error.downcast_ref::<reqwest::Error>().is_some() {
        return "管理画面との通信を完了できません。\n接続先とTailscaleの接続を確認してください。"
            .into();
    }
    if let Some(issue) = error.downcast_ref::<LoginIssue>() {
        return match issue {
            LoginIssue::EmptyPassword => "パスワードが空です。\nパスワード欄へ入力して、ログインをやり直してください。".into(),
            LoginIssue::EmptyUsername => "ユーザー名が空です。\n--usernameで本体のアカウント名を指定してください。".into(),
            LoginIssue::TooLong | LoginIssue::Denied(422) => "本体の入力条件に一致しません。\nHT813の管理者パスワードは1〜30文字です。\n入力した長さと文字種を確認してください。".into(),
            LoginIssue::Denied(401 | 403) => "本体が認証情報を受け付けませんでした。\n本体に設定したユーザー名とパスワードを確認してください。".into(),
            LoginIssue::Denied(status) => format!("本体へのログインを確認できませんでした。\n応答コード: {status}"),
        };
    }
    if let Some(api) = error.downcast_ref::<ApiFailure>() {
        return match api.code.as_str() {
            "authentication_required" => "ログインの有効期限が切れています。\nログインをやり直してください。".into(),
            "device_unavailable" => "HT813に接続できません。\n接続先と本体の電源を確認してください。".into(),
            "unsupported_form" => "本体の入力画面に未対応の処理があります。\n対応状況を確認してから変更してください。".into(),
            "stale_revision" => "変更案の作成後に本体の設定が変わりました。\n現在の設定を取得して、変更案を作成してください。".into(),
            "invalid_change" => "本体の入力条件に一致しません。\n入力値と項目の説明を確認してください。".into(),
            "device_busy" => "HT813は別の処理を実行しています。\n処理の終了後に状態を確認してください。".into(),
            "access_denied" => "管理画面への接続が許可されていません。\nTailscaleの管理用接続を確認してください。".into(),
            "session_limit" => "管理画面のログイン数が上限に達しています。\n使用していない画面からログアウトしてください。".into(),
            _ => "管理画面から受け取った応答を確認できません。".into(),
        };
    }
    error.to_string()
}
