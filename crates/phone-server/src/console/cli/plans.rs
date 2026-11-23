use super::{Api, MAX_INPUT, Output, Session, files, private_json};
use crate::console::model::*;
use anyhow::{Context as _, ensure};
use base64::Engine as _;
use hmac::{Hmac, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
};
use url::Url;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(default)]
    values: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    secrets: BTreeMap<String, PathBuf>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Value {
    Public { values: Vec<String> },
    SecretFile { path: PathBuf },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Change {
    id: String,
    parameter: String,
    label: String,
    field_kind: FieldKind,
    before: Vec<String>,
    value: Value,
}
impl Change {
    fn secret(&self) -> bool {
        matches!(self.value, Value::SecretFile { .. })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Body {
    id: Uuid,
    origin: Url,
    page: PageId,
    revision: String,
    mode: WriteMode,
    changes: Vec<Change>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: u8,
    body: Body,
    seal: String,
    inputs_seal: String,
}

type Values = BTreeMap<String, Vec<String>>;

fn mac(state: &Session, body: &Body, values: Option<&Values>) -> anyhow::Result<Hmac<Sha256>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(state.cookie.as_bytes())
        .map_err(|_| anyhow::anyhow!("invalid plan authentication key"))?;
    mac.update(b"ht813-console-plan\0");
    let bytes = serde_json::to_vec(body)?;
    mac.update(&(bytes.len() as u64).to_be_bytes());
    mac.update(&bytes);
    if let Some(values) = values {
        mac.update(b"inputs\0");
        let bytes = serde_json::to_vec(values)?;
        mac.update(&(bytes.len() as u64).to_be_bytes());
        mac.update(&bytes);
    }
    Ok(mac)
}

fn seal(state: &Session, body: &Body, values: Option<&Values>) -> anyhow::Result<String> {
    Ok(base64::engine::general_purpose::STANDARD
        .encode(mac(state, body, values)?.finalize().into_bytes()))
}

fn verify(state: &Session, body: &Body, values: Option<&Values>, seal: &str) -> anyhow::Result<()> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(seal)
        .map_err(|_| anyhow::anyhow!("invalid plan authentication code"))?;
    mac(state, body, values)?.verify_slice(&bytes).map_err(|_| {
        anyhow::anyhow!("plan, session, or private inputs changed; create a fresh plan")
    })
}

impl Plan {
    fn read(state: &Session, path: &Path) -> anyhow::Result<Self> {
        let plan: Self = private_json(path)?;
        ensure!(
            plan.schema == 1
                && plan.body.origin == state.origin
                && !plan.body.id.is_nil()
                && plan.body.page != PageId::Status
                && !plan.body.revision.is_empty()
                && !plan.body.changes.is_empty()
                && plan.body.changes.len() <= 1024,
            "invalid reviewed plan identity"
        );
        verify(state, &plan.body, None, &plan.seal)?;
        Ok(plan)
    }
}

fn values(body: &Body) -> anyhow::Result<Values> {
    let mut values = Values::new();
    for change in &body.changes {
        let entry = match &change.value {
            Value::Public { values } => values.clone(),
            Value::SecretFile { path } => {
                ensure!(path.is_absolute(), "secret file paths must be absolute");
                vec![files::secret(files::read(path, 16 * 1024)?)?]
            }
        };
        ensure!(
            values.insert(change.id.clone(), entry).is_none(),
            "duplicate planned control"
        );
    }
    Ok(values)
}

fn planned(snapshot: &PageSnapshot, input: Input) -> anyhow::Result<Vec<Change>> {
    ensure!(
        !input
            .values
            .keys()
            .any(|parameter| input.secrets.contains_key(parameter)),
        "a parameter must have exactly one input source"
    );
    let mut changes = vec![];
    let requested = input
        .values
        .into_iter()
        .map(|(key, values)| (key, Value::Public { values }))
        .chain(
            input
                .secrets
                .into_iter()
                .map(|(key, path)| (key, Value::SecretFile { path })),
        );
    for (parameter, value) in requested {
        let setting = snapshot
            .settings
            .iter()
            .find(|setting| setting.field.parameter == parameter)
            .context("requested parameter is absent from the page")?;
        let field = &setting.field;
        ensure!(
            !field.disabled && !field.read_only && setting.help.is_some(),
            "requested parameter is not editable"
        );
        ensure!(
            field.secret == matches!(value, Value::SecretFile { .. }),
            "secret controls require private secret files and public controls require public values"
        );
        if matches!(&value, Value::Public { values } if *values == field.values) {
            continue;
        }
        changes.push(Change {
            id: field.id.clone(),
            parameter,
            label: field.label.clone(),
            field_kind: field.kind,
            before: field.values.clone(),
            value,
        });
    }
    ensure!(
        !changes.is_empty(),
        "the plan contains no effective changes"
    );
    Ok(changes)
}

#[derive(Serialize)]
struct ReviewedChange<'a> {
    parameter: &'a str,
    label: &'a str,
    before: Option<&'a [String]>,
    after: Option<&'a [String]>,
    secret: bool,
}

#[derive(Serialize)]
struct Review<'a> {
    plan_id: Uuid,
    origin: &'a Url,
    page: PageId,
    revision: &'a str,
    mode: WriteMode,
    changes: Vec<ReviewedChange<'a>>,
}
impl<'a> From<&'a Body> for Review<'a> {
    fn from(body: &'a Body) -> Self {
        Self {
            plan_id: body.id,
            origin: &body.origin,
            page: body.page,
            revision: &body.revision,
            mode: body.mode,
            changes: body
                .changes
                .iter()
                .map(|change| ReviewedChange {
                    parameter: &change.parameter,
                    label: &change.label,
                    before: if change.secret() {
                        None
                    } else {
                        Some(&change.before)
                    },
                    after: match &change.value {
                        Value::Public { values } => Some(values),
                        Value::SecretFile { .. } => None,
                    },
                    secret: change.secret(),
                })
                .collect(),
        }
    }
}

pub(super) async fn create(
    state: Session,
    page: PageId,
    changes: &Path,
    mode: WriteMode,
    output: &Path,
    display: &Output,
    session_path: &Path,
) -> anyhow::Result<()> {
    let output = files::new_path(output)?;
    ensure!(page != PageId::Status, "status has no editable settings");
    let source = String::from_utf8(files::read(changes, MAX_INPUT)?)
        .map_err(|_| anyhow::anyhow!("change input is not UTF-8"))?;
    let input: Input =
        toml::from_str(&source).map_err(|_| anyhow::anyhow!("invalid change document"))?;
    let api = Api::authenticated(&state)?;
    display.activity("変更案を本体の入力条件と照合しています");
    let snapshot = api.page(page).await?;
    ensure!(
        !snapshot.reconciliation_required
            && match mode {
                WriteMode::Save => snapshot.can_save,
                WriteMode::SaveAndApply => snapshot.can_apply,
            },
        "the native page does not permit the requested operation"
    );
    let body = Body {
        id: Uuid::new_v4(),
        origin: state.origin.clone(),
        page,
        revision: snapshot.revision.clone(),
        mode,
        changes: planned(&snapshot, input)?,
    };
    let values = values(&body)?;
    let review: WriteReview = api
        .post(
            &format!("pages/{}/review", page.key()),
            &Changes {
                revision: body.revision.clone(),
                mode,
                values: values.clone(),
            },
        )
        .await?;
    ensure!(
        review.page == page
            && review.revision == body.revision
            && review.mode == mode
            && review.changes == body.changes.len()
            && review.secret_changes
                == body.changes.iter().filter(|change| change.secret()).count(),
        "server validation does not match the reviewed changes"
    );
    let plan = Plan {
        schema: 1,
        seal: seal(&state, &body, None)?,
        inputs_seal: seal(&state, &body, Some(&values))?,
        body,
    };
    files::write(&output, &serde_json::to_vec_pretty(&plan)?)?;
    display.emit(&Review::from(&plan.body), |screen| {
        screen.heading("○ 変更案を確認しました");
        screen.line("本体への送信は、適用コマンドで行います。");
        for change in &plan.body.changes {
            let value = match &change.value {
                Value::SecretFile { .. } => "非公開値を変更".to_owned(),
                Value::Public { values } => {
                    format!("{}\n→ {}", change.before.join(" / "), values.join(" / "))
                }
            };
            screen.field(&format!("{} · {}", change.parameter, change.label), &value);
        }
        screen.field(
            "適用方法",
            match mode {
                WriteMode::Save => "保存",
                WriteMode::SaveAndApply => "保存して反映",
            },
        );
        screen.field(
            "次の操作",
            &display.command(&[
                "apply",
                "--session",
                &session_path.to_string_lossy(),
                "--plan",
                &output.to_string_lossy(),
            ]),
        );
    })
}

fn matches(body: &Body, snapshot: &PageSnapshot) -> bool {
    body.changes.iter().all(|change| {
        snapshot.settings.iter().any(|setting| {
            setting.field.parameter == change.parameter
                && setting.field.kind == change.field_kind
                && setting.field.secret == change.secret()
                && match &change.value {
                    Value::Public { values } => setting.field.values == *values,
                    Value::SecretFile { .. } => true,
                }
        })
    })
}

fn receipt_valid(body: &Body, receipt: &WriteReceipt) -> anyhow::Result<()> {
    if let Some(snapshot) = &receipt.snapshot {
        ensure!(
            snapshot.page == body.page,
            "readback belongs to another page"
        );
        super::validate_snapshot(snapshot)?;
    }
    match receipt.outcome {
        WriteOutcome::Confirmed | WriteOutcome::VerificationRequired => {
            ensure!(
                receipt
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| matches(body, snapshot)),
                "confirmed readback does not match the plan"
            );
            ensure!(
                (receipt.outcome == WriteOutcome::VerificationRequired)
                    == body.changes.iter().any(Change::secret),
                "readback omitted independent secret verification"
            );
        }
        WriteOutcome::Uncertain => {}
    }
    Ok(())
}

pub(super) async fn apply(
    state: Session,
    path: &Path,
    display: &Output,
    session_path: &Path,
) -> anyhow::Result<ExitCode> {
    let plan = Plan::read(&state, path)?;
    let values = values(&plan.body)?;
    verify(&state, &plan.body, Some(&values), &plan.inputs_seal)?;
    files::claim(&state.claims, plan.body.id)?;
    display.activity("設定の送信と保存内容の確認を行っています");
    let api = Api::authenticated(&state)?;
    let result: anyhow::Result<WriteReceipt> = api
        .post(
            &format!("pages/{}", plan.body.page.key()),
            &Changes {
                revision: plan.body.revision.clone(),
                mode: plan.body.mode,
                values,
            },
        )
        .await;
    let receipt = match result {
        Ok(receipt) => match receipt_valid(&plan.body, &receipt) {
            Ok(()) => receipt,
            Err(error) => {
                display.diagnostic(&error)?;
                WriteReceipt {
                    outcome: WriteOutcome::Uncertain,
                    snapshot: None,
                }
            }
        },
        Err(error) => {
            display.diagnostic(&error)?;
            WriteReceipt {
                outcome: WriteOutcome::Uncertain,
                snapshot: None,
            }
        }
    };
    display.emit(&receipt, |screen| {
        screen.heading(match receipt.outcome {
            WriteOutcome::Confirmed => "✓ 保存内容を確認しました",
            WriteOutcome::VerificationRequired => "! 接続確認が必要です",
            WriteOutcome::Uncertain => "! 適用結果の確認が必要です",
        });
        match receipt.outcome {
            WriteOutcome::Confirmed => screen.line("公開値の読み戻しが変更案と一致しました。"),
            WriteOutcome::VerificationRequired => screen.line("公開値の読み戻しを確認しました。\n非公開値は、接続先で認証と通話を確認してください。"),
            WriteOutcome::Uncertain => screen.line("送信済みの変更案は使用済みです。\n現在の設定を取得して、適用結果を確認してください。"),
        }
        if receipt.outcome == WriteOutcome::Uncertain {
            screen.field("確認操作", &display.command(&["reconcile", "--session", &session_path.to_string_lossy(), "--plan", &path.to_string_lossy()]));
        }
    })?;
    Ok(match receipt.outcome {
        WriteOutcome::Confirmed => ExitCode::SUCCESS,
        WriteOutcome::VerificationRequired => ExitCode::from(2),
        WriteOutcome::Uncertain => ExitCode::from(3),
    })
}

pub(super) async fn reconcile(
    state: Session,
    path: &Path,
    display: &Output,
) -> anyhow::Result<ExitCode> {
    let plan = Plan::read(&state, path)?;
    display.activity("本体の現在値と変更案を比較しています");
    let snapshot = Api::authenticated(&state)?.page(plan.body.page).await?;
    let public_values_match = matches(&plan.body, &snapshot);
    let secret_verification_required = plan.body.changes.iter().any(Change::secret);
    #[derive(Serialize)]
    struct Reconciliation {
        plan_id: Uuid,
        public_values_match: bool,
        secret_verification_required: bool,
        snapshot: PageSnapshot,
    }
    display.emit(
        &Reconciliation {
            plan_id: plan.body.id,
            public_values_match,
            secret_verification_required,
            snapshot,
        },
        |screen| {
            screen.heading(if !public_values_match {
                "! 本体の設定と変更案が一致しません"
            } else if secret_verification_required {
                "! 非公開値の確認が必要です"
            } else {
                "✓ 設定が一致しています"
            });
            screen.field(
                "公開値",
                if public_values_match {
                    "変更案と一致"
                } else {
                    "変更案と不一致"
                },
            );
            if secret_verification_required {
                screen.field("非公開値の確認", "接続先で認証と通話を確認してください。");
            }
            screen.line("本体の動作と通話は、実機で確認してください。");
        },
    )?;
    Ok(if !public_values_match {
        ExitCode::from(3)
    } else if secret_verification_required {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}
