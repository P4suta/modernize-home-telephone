use phone_core::console::Mode;
use phone_server::console::{
    model::FieldKind,
    native::{NativeError, NativeForm},
};
use std::collections::BTreeMap;
use url::Url;

#[test]
fn every_observed_form_field_remains_documented() {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Inventory {
        firmware: String,
        pages: BTreeMap<String, Vec<String>>,
    }
    let inventory: Inventory = toml::from_str(include_str!(
        "../../../verification/ht813-console-fields.toml"
    ))
    .unwrap();
    assert_eq!(inventory.firmware, "1.0.19.6");
    assert_eq!(inventory.pages.len(), 4);
    let catalogue = phone_server::console::documentation::catalogue();
    for (page, count) in [
        ("basic", 133),
        ("advanced", 123),
        ("fxs", 183),
        ("fxo", 158),
    ] {
        let fields = &inventory.pages[page];
        assert_eq!(fields.len(), count);
        assert_eq!(
            fields
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            count
        );
        for parameter in fields {
            assert_eq!(
                catalogue
                    .iter()
                    .filter(|help| help.parameters.contains(parameter))
                    .count(),
                1,
                "missing or duplicated help for {page} {parameter}"
            );
        }
    }
}

#[test]
fn firmware_help_rejects_duplicate_missing_or_misrepresented_sources() {
    use phone_server::console::{
        documentation::append_firmware_help,
        model::{Help, HelpReference},
    };
    let firmware: Vec<Help> = serde_json::from_str(include_str!(
        "../../../console/src/lib/firmware-parameters.json"
    ))
    .unwrap();
    assert_eq!(firmware.len(), 10);
    assert!(append_firmware_help(vec![], &firmware).is_ok());
    assert!(append_firmware_help(vec![], &[]).is_err());
    assert!(append_firmware_help(firmware.clone(), &firmware).is_err());
    let mut duplicated = firmware.clone();
    duplicated.push(firmware[0].clone());
    assert!(append_firmware_help(vec![], &duplicated).is_err());
    for mutate in 0..4 {
        let mut altered = firmware.clone();
        match mutate {
            0 => altered[0].template_value = Some("invented-template-fixture".into()),
            1 => {
                altered[0].reference = HelpReference::Template {
                    firmware: "1.0.17.3".into(),
                    line: 1,
                }
            }
            2 => altered[0].summary.clear(),
            _ => altered[0].ranges[0].min = 99,
        }
        assert!(append_firmware_help(vec![], &altered).is_err());
    }
}

#[test]
fn observed_firmware_parameters_have_help_without_invented_template_values() {
    let catalogue = phone_server::console::documentation::catalogue();
    for parameter in [
        "P28772", "P2383", "P4429", "P26140", "P28773", "P2485", "P2483", "P28174", "P28178",
        "P8723",
    ] {
        let help = catalogue
            .iter()
            .find(|help| help.parameters.iter().any(|id| id == parameter))
            .expect("observed parameter explanation is missing");
        assert!(help.template_value.is_none());
        assert!(
            !help.summary.is_empty() && !help.input.is_empty() && !help.recommendation.is_empty()
        );
    }
    let html = "<form method='post' action='update'><input type='hidden' name='csrf_token' value='fixture'><input name='P8467' value='0'><input type='submit' name='update' value='Update'></form>";
    let form = NativeForm::parse(html, &document("config")).unwrap();
    assert!(
        phone_server::console::documentation::find(
            &catalogue,
            &form,
            &form.fields()[0].id,
            "Download and Process ALL Available Config Files"
        )
        .is_some()
    );
}

#[test]
fn changed_web_passwords_must_remain_encodable_by_native_login() {
    for parameter in ["P2", "P196", "P28113"] {
        let html = format!(
            "<form method='post' action='update'><input type='hidden' name='csrf_token' value='fixture'><input type='password' name='{parameter}' maxlength='30'><input type='submit' name='update' value='Update'></form>"
        );
        let form = NativeForm::parse(&html, &document("config")).unwrap();
        let id = form.fields()[0].id.clone();
        assert!(
            form.settings_values(
                &BTreeMap::from([(id.clone(), vec!["pássword".into()])]),
                Mode::Save
            )
            .is_ok()
        );
        assert_eq!(
            form.settings_values(
                &BTreeMap::from([(id, vec!["パスワード".into()])]),
                Mode::Save
            ),
            Err(NativeError::InvalidChange)
        );
    }
}

#[test]
fn submitters_preserve_their_position_before_trailing_hidden_controls() {
    let html = "<form method='post' action='update'><input type='hidden' name='session_token' value='fixture'><input name='P40' value='5060'><input type='submit' name='update' value='Update'><input type='submit' name='apply' value='Apply'><input type='hidden' name='gnkey' value='fixture-key'></form>";
    let form = NativeForm::parse(html, &document("config")).unwrap();
    for (mode, button, value) in [
        (Mode::Save, "update", "Update"),
        (Mode::SaveAndApply, "apply", "Apply"),
    ] {
        assert_eq!(
            form.settings_values(&BTreeMap::new(), mode).unwrap(),
            vec![
                ("session_token".into(), "fixture".into()),
                ("P40".into(), "5060".into()),
                (button.into(), value.into()),
                ("gnkey".into(), "fixture-key".into()),
            ]
        );
    }
}

#[test]
fn secret_choice_values_remain_private_and_cannot_be_edited_as_public_options() {
    for control in [
        "<select name='P34'><option value='private-option-fixture'>Credential</option></select>",
        "<input name='P34' type='radio' value='private-option-fixture' checked>A<input name='P34' type='radio' value='other-private-fixture'>B",
    ] {
        let html = format!(
            "<form method='post' action='update'><input type='hidden' name='csrf_token' value='fixture'><table><tr><td>Credential</td><td>{control}</td></tr></table><input type='submit' name='update' value='Update'></form>"
        );
        let form = NativeForm::parse(&html, &document("config")).unwrap();
        let field = &form.fields()[0];
        assert!(
            field.secret && field.values.is_empty() && field.choices.is_empty() && field.read_only
        );
        assert!(
            form.settings_values(
                &BTreeMap::from([("field-0".into(), vec!["private-option-fixture".into()])]),
                Mode::Save
            )
            .is_err()
        );
        assert!(
            form.settings_values(&BTreeMap::new(), Mode::Save)
                .unwrap()
                .contains(&("P34".into(), "private-option-fixture".into()))
        );
    }
}

#[test]
fn every_preserved_parameter_has_distinct_help_with_correct_polarity_and_ranges() {
    let catalogue = phone_server::console::documentation::catalogue();
    assert_eq!(
        catalogue
            .iter()
            .filter(|help| matches!(
                help.reference,
                phone_server::console::model::HelpReference::Template { .. }
            ))
            .count(),
        598
    );
    assert_eq!(catalogue.len(), 608);
    let find = |id: &str| {
        catalogue
            .iter()
            .find(|help| help.parameters == [id])
            .unwrap()
    };
    assert!(find("P16").title.contains("サブネットマスク"));
    assert!(find("P731").title.contains("登録"));
    assert!(find("P91").title.contains("禁止"));
    assert!(find("P28116").title.contains("タイムアウト"));
    assert!(find("P4420").title.contains("時間上限"));
    let value = serde_json::to_value(find("P39")).unwrap();
    assert_eq!(
        value["ranges"],
        serde_json::json!([{"min":1024,"max":65535}])
    );
    let disputed = serde_json::to_value(find("P144")).unwrap();
    assert_eq!(disputed["template_value"], "1");
    assert!(
        disputed["default_value"]
            .as_str()
            .unwrap()
            .starts_with("いいえ")
    );
}

#[test]
fn private_keys_and_plain_text_credentials_are_never_disclosed() {
    for (name, label, control) in [
        (
            "P28113",
            "Credential",
            "<input name='P28113' value='viewer-fixture'>",
        ),
        (
            "P8606",
            "Credential",
            "<textarea name='P8606'>key-fixture</textarea>",
        ),
        (
            "P279",
            "SIP TLS Private Key",
            "<textarea name='P279'>private-key-fixture</textarea>",
        ),
        (
            "P734",
            "Authenticate Password",
            "<input name='P734' value='private-password-fixture'>",
        ),
        (
            "P21902",
            "SNMPv1/v2c Community",
            "<input name='P21902' value='community-fixture'>",
        ),
        (
            "future-secret",
            "Shared Secret",
            "<input name='future-secret' value='future-secret-fixture'>",
        ),
    ] {
        let html = format!(
            "<form method='post' action='update'><input type='hidden' name='csrf_token' value='fixture'><table><tr><td>{label}</td><td>{control}</td></tr></table><input type='submit' name='update' value='Update'></form>"
        );
        let form = NativeForm::parse(&html, &document("config")).unwrap();
        assert_eq!(form.fields()[0].parameter, name);
        assert!(form.fields()[0].secret);
        assert!(form.fields()[0].values.is_empty());
        assert!(
            form.settings_values(
                &BTreeMap::from([("field-0".into(), vec![String::new()])]),
                Mode::Save
            )
            .is_err()
        );
        assert!(
            form.settings_values(&BTreeMap::new(), Mode::Save)
                .unwrap()
                .iter()
                .any(|(key, _)| key == name)
        );
    }
}

#[test]
fn native_certificates_accept_the_documented_size_without_exposing_private_keys() {
    let certificate = "a".repeat(8192);
    let html = format!(
        "<form method='post' action='update'><input type='hidden' name='csrf_token' value='fixture'><table><tr><td>CPE SSL Certificate</td><td><textarea name='P8220' maxlength='8192'>{certificate}</textarea></td></tr></table><input type='submit' name='update' value='Update'></form>"
    );
    let parsed = NativeForm::parse(&html, &document("config")).unwrap();
    assert_eq!(
        parsed.fields()[0].values.as_slice(),
        std::slice::from_ref(&certificate)
    );
    let values = parsed
        .settings_values(
            &BTreeMap::from([("field-0".into(), vec![certificate.clone()])]),
            Mode::Save,
        )
        .unwrap();
    assert!(values.contains(&("P8220".into(), certificate)));
}

#[test]
fn unwrapped_radio_labels_preserve_negative_setting_semantics() {
    let html=SETTINGS.replace("<label><input name=\"enabled\" type=\"radio\" value=\"1\" checked>Yes</label><label><input name=\"enabled\" type=\"radio\" value=\"0\">No</label>",
        "<input name=\"enabled\" type=\"radio\" value=\"1\" checked>Yes <input name=\"enabled\" type=\"radio\" value=\"0\">No")
        .replace("Account Active","Disable Call Waiting");
    let parsed = NativeForm::parse(&html, &document("config")).unwrap();
    assert_eq!(
        parsed.fields()[1]
            .choices
            .iter()
            .map(|choice| choice.label.as_str())
            .collect::<Vec<_>>(),
        ["はい", "いいえ"]
    );
}

#[test]
fn unverified_form_metadata_is_readable_but_never_writable() {
    for html in [
        SETTINGS.replace("method=\"post\"", "method=\"get\""),
        SETTINGS.replace(
            "<input type=\"hidden\" name=\"csrf_token\" value=\"csrf-fixture\">",
            "",
        ),
        SETTINGS.replace(
            "<option value=\"8\">PCMA</option>",
            "<option value=\"0\">Same value alias</option>",
        ),
    ] {
        assert!(NativeForm::parse(&html, &document("config")).is_err());
        let parsed = NativeForm::read(&html, &document("config")).unwrap();
        assert_eq!(parsed.fields().len(), 7);
        assert!(parsed.fields().iter().all(|field| field.read_only));
        assert!(!parsed.supports_mode(Mode::Save) && !parsed.supports_mode(Mode::SaveAndApply));
        assert_eq!(
            parsed.settings_values(
                &BTreeMap::from([("field-0".into(), vec!["changed".into()])]),
                Mode::Save
            ),
            Err(NativeError::UnsupportedForm)
        );
        assert!(parsed.fields()[6].values.is_empty());
    }
}

mod http_contract {
    use super::{LOGIN, SETTINGS};
    use axum::{
        Form, Router,
        extract::State,
        http::{HeaderMap, StatusCode, header},
        response::IntoResponse,
        routing::{get, post},
    };
    use phone_server::console::http::{Config, router};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use tokio::{net::TcpListener, task::JoinHandle};
    use url::Url;

    #[derive(Default)]
    struct Device {
        generation: usize,
        logins: usize,
        writes: usize,
        server: Option<String>,
        secret: Option<String>,
        ignore_server: bool,
        replace_parameter_after_write: bool,
        fail_after_write: bool,
        token: usize,
    }
    type Shared = Arc<Mutex<Device>>;

    #[tokio::test]
    async fn cli_login_reads_and_revokes_its_own_private_session_without_a_browser() {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let fixture = Fixture::start().await;
        let temporary = tempfile::tempdir().unwrap();
        let session = temporary.path().join("session.json");
        let password = temporary.path().join("password");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&password)
            .unwrap()
            .write_all(b"secret\n")
            .unwrap();
        let login = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["login", "--origin", &fixture.origin, "--session"])
            .arg(&session)
            .arg("--password-file")
            .arg(&password)
            .output()
            .await
            .unwrap();
        assert!(
            login.status.success(),
            "{}",
            String::from_utf8_lossy(&login.stderr)
        );
        assert!(!String::from_utf8_lossy(&login.stdout).contains("secret"));
        let stored = std::fs::read_to_string(&session).unwrap();
        assert!(!stored.contains("secret"));
        assert!(!stored.contains("password"));
        let get = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["get", "--session"])
            .arg(&session)
            .args(["--page", "status"])
            .output()
            .await
            .unwrap();
        assert!(
            get.status.success(),
            "{}",
            String::from_utf8_lossy(&get.stderr)
        );
        let snapshot: Value = serde_json::from_slice(&get.stdout).unwrap();
        assert_eq!(snapshot["page"], "status");
        assert_eq!(snapshot["information"][1]["label"], "FXS: 受話器");
        let logout = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["logout", "--session"])
            .arg(&session)
            .output()
            .await
            .unwrap();
        assert!(logout.status.success());
        assert!(!session.exists());
    }

    #[tokio::test]
    async fn review_checks_native_values_without_submitting_or_consuming_the_revision() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let baseline = fixture.basic().await;
        let changes = json!({"revision":baseline["revision"],"mode":"save","values":{"field-0":["sip.changed.invalid"],"field-6":["review-secret-fixture"]}});
        let response = fixture
            .client
            .post(fixture.path("pages/basic/review"))
            .header("Origin", &fixture.origin)
            .json(&changes)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let review: Value = response.json().await.unwrap();
        assert_eq!(review["changes"], 2);
        assert_eq!(review["secret_changes"], 1);
        assert!(!review.to_string().contains("review-secret-fixture"));
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        let invalid = fixture.client.post(fixture.path("pages/basic/review"))
            .header("Origin", &fixture.origin)
            .json(&json!({"revision":baseline["revision"],"mode":"save","values":{"field-1":["invalid-choice"]}}))
            .send().await.unwrap();
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            fixture.write(&baseline["revision"]).await.status(),
            StatusCode::OK
        );
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
    }

    #[tokio::test]
    async fn matching_public_readback_still_requires_independent_secret_verification() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let baseline = fixture.basic().await;
        let response = fixture.client.post(fixture.path("pages/basic")).header("Origin", &fixture.origin)
            .json(&json!({"revision":baseline["revision"],"mode":"save","values":{"field-0":["sip.changed.invalid"],"field-6":["secret-changed-fixture"]}}))
            .send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: Value = response.json().await.unwrap();
        assert_eq!(receipt["outcome"], "verification_required");
        assert!(!receipt.to_string().contains("secret-changed-fixture"));
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
    }

    #[tokio::test]
    async fn a_secret_change_cannot_hide_an_unapplied_public_change() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let baseline = fixture.basic().await;
        fixture.device.lock().unwrap().ignore_server = true;
        let response = fixture.client.post(fixture.path("pages/basic")).header("Origin", &fixture.origin)
            .json(&json!({"revision":baseline["revision"],"mode":"save","values":{"field-0":["sip.changed.invalid"],"field-6":["secret-changed-fixture"]}}))
            .send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: Value = response.json().await.unwrap();
        assert_eq!(receipt["outcome"], "uncertain");
        assert!(!receipt.to_string().contains("secret-changed-fixture"));
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
    }

    #[tokio::test]
    async fn readback_cannot_confirm_values_bound_to_a_different_native_parameter() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let baseline = fixture.basic().await;
        fixture.device.lock().unwrap().replace_parameter_after_write = true;
        let response = fixture.write(&baseline["revision"]).await;
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: Value = response.json().await.unwrap();
        assert_eq!(receipt["outcome"], "uncertain");
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
    }

    #[tokio::test]
    async fn a_review_from_an_earlier_browser_session_cannot_authorize_a_write() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let earlier = fixture.basic().await;
        fixture
            .client
            .delete(fixture.path("session"))
            .header("Origin", &fixture.origin)
            .send()
            .await
            .unwrap();
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let current = fixture.basic().await;
        assert_ne!(earlier["revision"], current["revision"]);
        assert_eq!(
            fixture.write(&earlier["revision"]).await.status(),
            StatusCode::CONFLICT
        );
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
    }

    #[tokio::test]
    async fn a_review_for_another_page_cannot_authorize_a_write() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let basic = fixture.basic().await;
        let fxo = fixture
            .client
            .get(fixture.path("pages/fxo"))
            .send()
            .await
            .unwrap();
        assert_eq!(fxo.status(), StatusCode::OK);
        let response=fixture.client.post(fixture.path("pages/fxo")).header("Origin",&fixture.origin)
            .json(&json!({"revision":basic["revision"],"mode":"save","values":{"field-0":["sip.changed.invalid"]}}))
            .send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
    }

    struct CliSession {
        directory: tempfile::TempDir,
        path: std::path::PathBuf,
    }
    impl CliSession {
        fn private(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
            use std::{io::Write as _, os::unix::fs::OpenOptionsExt as _};
            let path = self.directory.path().join(name);
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap()
                .write_all(bytes)
                .unwrap();
            path
        }
        async fn start(fixture: &Fixture) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("session.json");
            let session = Self { directory, path };
            let password = session.private("administrator", b"secret\n");
            let login = session
                .run(
                    "login",
                    &[
                        "--origin".into(),
                        fixture.origin.clone(),
                        "--password-file".into(),
                        password.display().to_string(),
                    ],
                )
                .await;
            assert!(
                login.status.success(),
                "{}",
                String::from_utf8_lossy(&login.stderr)
            );
            session
        }
        async fn run(&self, command: &str, arguments: &[String]) -> std::process::Output {
            tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
                .kill_on_drop(true)
                .args([command, "--session"])
                .arg(&self.path)
                .args(arguments)
                .output()
                .await
                .unwrap()
        }
        async fn plan(
            &self,
            source: &str,
            name: &str,
        ) -> (std::path::PathBuf, std::process::Output) {
            let changes = self.private(&format!("{name}.toml"), source.as_bytes());
            let output = self.directory.path().join(format!("{name}.json"));
            let result = self
                .run(
                    "plan",
                    &[
                        "--page".into(),
                        "basic".into(),
                        "--changes".into(),
                        changes.display().to_string(),
                        "--mode".into(),
                        "save".into(),
                        "--output".into(),
                        output.display().to_string(),
                    ],
                )
                .await;
            (output, result)
        }
        async fn apply(&self, path: &std::path::Path) -> std::process::Output {
            self.run("apply", &["--plan".into(), path.display().to_string()])
                .await
        }
    }

    #[tokio::test]
    async fn cli_public_plans_are_validated_single_use_and_reconciled() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let (plan, review) = session
            .plan("[values]\nserver = [\"sip.changed.invalid\"]\n", "public")
            .await;
        assert!(
            review.status.success(),
            "{}",
            String::from_utf8_lossy(&review.stderr)
        );
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        let review: Value = serde_json::from_slice(&review.stdout).unwrap();
        assert_eq!(review["changes"][0]["parameter"], "server");
        assert_eq!(
            review["changes"][0]["before"],
            json!(["sip.example.invalid"])
        );
        assert_eq!(
            review["changes"][0]["after"],
            json!(["sip.changed.invalid"])
        );
        let (left, right) = tokio::join!(session.apply(&plan), session.apply(&plan));
        assert_eq!(
            [&left, &right]
                .iter()
                .filter(|output| output.status.success())
                .count(),
            1
        );
        let (write, refused) = if left.status.success() {
            (left, right)
        } else {
            (right, left)
        };
        assert_eq!(refused.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&refused.stderr).contains("already submitted"));
        assert_eq!(
            serde_json::from_slice::<Value>(&write.stdout).unwrap()["outcome"],
            "confirmed"
        );
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        assert!(!session.apply(&plan).await.status.success());
        let copy = session.private("copied-plan.json", &std::fs::read(&plan).unwrap());
        assert!(!session.apply(&copy).await.status.success());
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        let reconciliation = session
            .run("reconcile", &["--plan".into(), plan.display().to_string()])
            .await;
        assert!(reconciliation.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&reconciliation.stdout).unwrap()["public_values_match"],
            true
        );
        let docs = session
            .run("docs", &["--parameter".into(), "P47".into()])
            .await;
        assert!(docs.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&docs.stdout).unwrap()[0]["key"],
            "parameter-p47"
        );
    }

    #[tokio::test]
    async fn cli_plan_and_private_secret_changes_cannot_authorize_an_unreviewed_write() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let secret = session.private("gateway-password", b"first-secret-fixture\n");
        let source = format!(
            "[values]\nserver = [\"sip.changed.invalid\"]\n[secrets]\nsecret = {:?}\n",
            secret.display().to_string()
        );
        let (path, review) = session.plan(&source, "secret").await;
        assert!(
            review.status.success(),
            "{}",
            String::from_utf8_lossy(&review.stderr)
        );
        let original = std::fs::read(&path).unwrap();
        for bytes in [&review.stdout, &original] {
            assert!(!String::from_utf8_lossy(bytes).contains("first-secret-fixture"));
        }
        let mut tampered: Value = serde_json::from_slice(&original).unwrap();
        tampered["body"]["changes"][0]["value"]["values"] = json!(["sip.unreviewed.invalid"]);
        std::fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(!session.apply(&path).await.status.success());
        std::fs::write(&path, &original).unwrap();
        std::fs::write(&secret, b"second-secret-fixture\n").unwrap();
        assert!(!session.apply(&path).await.status.success());
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        std::fs::write(&secret, b"first-secret-fixture\n").unwrap();
        let applied = session.apply(&path).await;
        assert_eq!(
            applied.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        assert!(!String::from_utf8_lossy(&applied.stdout).contains("first-secret-fixture"));
        assert_eq!(
            serde_json::from_slice::<Value>(&applied.stdout).unwrap()["outcome"],
            "verification_required"
        );
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        std::fs::remove_file(&secret).unwrap();
        let reconciled = session
            .run("reconcile", &["--plan".into(), path.display().to_string()])
            .await;
        assert_eq!(reconciled.status.code(), Some(2));
        let reconciled: Value = serde_json::from_slice(&reconciled.stdout).unwrap();
        assert_eq!(reconciled["public_values_match"], true);
        assert_eq!(reconciled["secret_verification_required"], true);
    }

    #[tokio::test]
    async fn cli_ambiguous_and_stale_writes_never_retry_or_report_success() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let (stale, result) = session
            .plan("[values]\nserver = [\"sip.changed.invalid\"]\n", "stale")
            .await;
        assert!(result.status.success());
        fixture.device.lock().unwrap().server = Some("sip.external-change.invalid".into());
        let refused = session.apply(&stale).await;
        assert_eq!(refused.status.code(), Some(3));
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        let (current, result) = session
            .plan("[values]\nserver = [\"sip.changed.invalid\"]\n", "current")
            .await;
        assert!(result.status.success());
        fixture.device.lock().unwrap().fail_after_write = true;
        let uncertain = session.apply(&current).await;
        assert_eq!(uncertain.status.code(), Some(3));
        assert_eq!(
            serde_json::from_slice::<Value>(&uncertain.stdout).unwrap()["outcome"],
            "uncertain"
        );
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        assert!(!session.apply(&current).await.status.success());
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        let reconciled = session
            .run(
                "reconcile",
                &["--plan".into(), current.display().to_string()],
            )
            .await;
        assert!(reconciled.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&reconciled.stdout).unwrap()["public_values_match"],
            true
        );
    }

    #[tokio::test]
    async fn cli_private_inputs_refuse_permissions_links_pipes_and_plaintext_secrets() {
        use std::os::unix::fs::PermissionsExt as _;
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        std::fs::set_permissions(&session.path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!session.run("get", &[]).await.status.success());
        std::fs::set_permissions(&session.path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(session.run("get", &[]).await.status.success());
        let password = session.directory.path().join("administrator");
        let link = session.directory.path().join("password-link");
        std::os::unix::fs::symlink(&password, &link).unwrap();
        let pipe = session.directory.path().join("password-pipe");
        assert!(
            std::process::Command::new("mkfifo")
                .args(["-m", "600"])
                .arg(&pipe)
                .status()
                .unwrap()
                .success()
        );
        let logins = fixture.device.lock().unwrap().logins;
        for input in [&link, &pipe] {
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
                    .kill_on_drop(true)
                    .args(["login", "--origin", &fixture.origin, "--session"])
                    .arg(session.directory.path().join("blocked-session.json"))
                    .arg("--password-file")
                    .arg(input)
                    .output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(!output.status.success());
            if input == &pipe {
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("bounded regular file"),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        assert_eq!(fixture.device.lock().unwrap().logins, logins);
        for (name, source) in [
            ("empty", "[values]\nserver = [\"sip.example.invalid\"]\n"),
            (
                "plaintext",
                "[values]\nsecret = [\"do-not-log-this-fixture\"]\n",
            ),
            ("unknown", "unknown = \"do-not-log-this-fixture\"\n"),
            ("invalid", "[values]\nsecret = [\"do-not-log-this-fixture\n"),
        ] {
            let (_, result) = session.plan(source, name).await;
            assert!(!result.status.success());
            assert!(!String::from_utf8_lossy(&result.stderr).contains("do-not-log-this-fixture"));
        }
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
    }

    #[tokio::test]
    async fn cli_credentials_do_not_follow_redirects_and_invalid_responses_cannot_leak_secrets() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let mut malformed = fixture.basic().await;
        malformed["settings"][6]["field"]["values"] = json!(["unexpected-sensitive-fixture"]);
        let sink = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed_sink = sink.clone();
        let fake = Router::new()
            .route("/api/console/session", post(|| async { (StatusCode::FOUND, [(header::LOCATION, "/credential-sink")]) }))
            .route("/credential-sink", get(move || {
                let observed_sink = observed_sink.clone();
                async move { observed_sink.fetch_add(1, std::sync::atomic::Ordering::SeqCst); StatusCode::OK }
            }))
            .route("/api/console/pages/basic", get(move || {
                let malformed = malformed.clone();
                async move { axum::Json(malformed) }
            }))
            .route("/api/console/pages/advanced", get(|| async {
                (StatusCode::BAD_GATEWAY, axum::Json(json!({"code":"unexpected-sensitive-fixture","message":"unexpected-sensitive-fixture"})))
            }));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, fake).await.unwrap();
        });
        let redirected = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["login", "--origin", &origin, "--session"])
            .arg(session.directory.path().join("redirect-session.json"))
            .arg("--password-file")
            .arg(session.directory.path().join("administrator"))
            .output()
            .await
            .unwrap();
        assert!(!redirected.status.success());
        assert_eq!(sink.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(
            !session
                .directory
                .path()
                .join("redirect-session.json")
                .exists()
        );
        let mut state: Value =
            serde_json::from_slice(&std::fs::read(&session.path).unwrap()).unwrap();
        state["origin"] = json!(origin);
        std::fs::write(&session.path, serde_json::to_vec(&state).unwrap()).unwrap();
        for page in ["basic", "advanced"] {
            let result = session.run("get", &["--page".into(), page.into()]).await;
            assert!(!result.status.success());
            assert!(result.stdout.is_empty());
            assert!(
                !String::from_utf8_lossy(&result.stderr).contains("unexpected-sensitive-fixture")
            );
        }
        task.abort();
    }

    #[tokio::test]
    async fn cli_sessions_bind_plans_and_accept_credentials_from_a_private_pipe() {
        use tokio::io::AsyncWriteExt as _;
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let (plan, reviewed) = session
            .plan("[values]\nserver = [\"sip.changed.invalid\"]\n", "bound")
            .await;
        assert!(reviewed.status.success());
        let other = CliSession::start(&fixture).await;
        let rejected = other.apply(&plan).await;
        assert!(!rejected.status.success());
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        let path = session.directory.path().join("pipe-session.json");
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["login", "--origin", &fixture.origin, "--session"])
            .arg(&path)
            .arg("--password-stdin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(b"secret\n").await.unwrap();
        drop(input);
        let result = child.wait_with_output().await.unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!String::from_utf8_lossy(&result.stdout).contains("secret"));
        assert!(!std::fs::read_to_string(path).unwrap().contains("secret"));
    }

    #[tokio::test]
    async fn cli_incomplete_login_revokes_an_issued_cookie_without_retaining_credentials() {
        let directory = tempfile::tempdir().unwrap();
        use std::{io::Write as _, os::unix::fs::OpenOptionsExt as _};
        let password = directory.path().join("password");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&password)
            .unwrap()
            .write_all(b"synthetic-password\n")
            .unwrap();
        let revocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = revocations.clone();
        let app = Router::new().route(
            "/api/console/session",
            post(|| async {
                (
                    StatusCode::OK,
                    [(
                        header::SET_COOKIE,
                        format!(
                            "homephone_console={}; Path=/api/console; HttpOnly; SameSite=Strict",
                            "a".repeat(64)
                        ),
                    )],
                    "invalid-session-body-with-sensitive-fixture",
                )
            })
            .delete(move |headers: HeaderMap| {
                let observed = observed.clone();
                async move {
                    assert_eq!(
                        headers.get(header::COOKIE).unwrap(),
                        &format!("homephone_console={}", "a".repeat(64))
                    );
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let session = directory.path().join("session.json");
        let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["login", "--origin", &origin, "--session"])
            .arg(&session)
            .arg("--password-file")
            .arg(&password)
            .output()
            .await
            .unwrap();
        assert!(!result.status.success());
        assert!(!session.exists());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("sensitive-fixture"));
        assert_eq!(revocations.load(std::sync::atomic::Ordering::SeqCst), 1);
        task.abort();
    }

    #[tokio::test]
    async fn cli_output_failures_return_errors_without_panics_or_replaying_writes() {
        let broken_output = || {
            let (reader, writer) = std::os::unix::net::UnixStream::pair().unwrap();
            drop(reader);
            std::process::Stdio::from(std::os::fd::OwnedFd::from(writer))
        };
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let (plan, result) = session
            .plan(
                "[values]\nserver = [\"sip.output-fixture.invalid\"]\n",
                "output",
            )
            .await;
        assert!(result.status.success());
        let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["apply", "--session"])
            .arg(&session.path)
            .arg("--plan")
            .arg(&plan)
            .stdout(broken_output())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"));
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        assert!(!session.apply(&plan).await.status.success());
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        let (plan, result) = session
            .plan(
                "[values]\nserver = [\"sip.error-fixture.invalid\"]\n",
                "error-output",
            )
            .await;
        assert!(result.status.success());
        fixture.device.lock().unwrap().server = Some("sip.external-fixture.invalid".into());
        let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["apply", "--session"])
            .arg(&session.path)
            .arg("--plan")
            .arg(&plan)
            .stderr(broken_output())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap()
            .wait_with_output()
            .await
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        assert!(!session.apply(&plan).await.status.success());
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
    }

    #[tokio::test]
    async fn cli_human_output_explains_the_workflow_and_preserves_machine_results() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let result = session
            .run("get", &["--human".into(), "--page".into(), "basic".into()])
            .await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let display = String::from_utf8(result.stdout).unwrap();
        assert!(display.contains("基本設定"));
        assert!(display.contains("SIPサーバー"));
        assert!(!display.contains("fixture-existing"));
        assert!(!display.contains("\"revision\""));
        assert!(!display.contains('\u{1b}'));
        let result = session
            .run("get", &["--json".into(), "--page".into(), "basic".into()])
            .await;
        assert!(result.status.success());
        let snapshot: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(snapshot["page"], "basic");
        let result = session
            .run(
                "docs",
                &["--human".into(), "--parameter".into(), "P47".into()],
            )
            .await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let display = String::from_utf8(result.stdout).unwrap();
        for label in ["入力", "推奨", "変更の影響"] {
            assert!(display.contains(label));
        }
        let secret = session.private("human-secret", b"human-secret-fixture\n");
        let changes = session.private(
            "human-changes.toml",
            format!(
                "[values]\nserver = [\"sip.human-fixture.invalid\"]\n[secrets]\nsecret = {:?}\n",
                secret.display().to_string()
            )
            .as_bytes(),
        );
        let plan = session.directory.path().join("human-plan.json");
        let result = session
            .run(
                "plan",
                &[
                    "--human".into(),
                    "--page".into(),
                    "basic".into(),
                    "--mode".into(),
                    "save".into(),
                    "--changes".into(),
                    changes.display().to_string(),
                    "--output".into(),
                    plan.display().to_string(),
                ],
            )
            .await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let display = String::from_utf8(result.stdout).unwrap();
        assert!(display.contains("変更案"));
        assert!(display.contains("sip.example.invalid"));
        assert!(display.contains("sip.human-fixture.invalid"));
        assert!(display.contains("非公開値を変更"));
        assert!(!display.contains("human-secret-fixture"));
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        let result = session
            .run(
                "apply",
                &[
                    "--human".into(),
                    "--plan".into(),
                    plan.display().to_string(),
                ],
            )
            .await;
        assert_eq!(
            result.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("接続確認が必要"));
        assert!(!String::from_utf8_lossy(&result.stdout).contains("human-secret-fixture"));
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        let result = session
            .run(
                "reconcile",
                &[
                    "--human".into(),
                    "--plan".into(),
                    plan.display().to_string(),
                ],
            )
            .await;
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stdout).contains("非公開値の確認"));
        let result = session.run("logout", &["--human".into()]).await;
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("ログアウト"));
        let password = session.private("human-password", b"secret\n");
        let path = session.directory.path().join("human-session.json");
        let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["login", "--human", "--origin", &fixture.origin, "--session"])
            .arg(&path)
            .arg("--password-file")
            .arg(&password)
            .output()
            .await
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let display = String::from_utf8(result.stdout).unwrap();
        assert!(display.contains("Authentication successful!"));
        assert!(display.contains("Hi admin!"));
        assert!(display.contains("次の操作"));
        assert!(!display.contains("\"authenticated\""));
        assert!(!display.contains("secret"));
    }

    #[tokio::test]
    async fn cli_human_output_neutralizes_terminal_controls_and_explains_empty_passwords() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        fixture.device.lock().unwrap().server =
            Some("sip.\u{1b}]52;c;dGVzdA==\u{7}.invalid".into());
        let result = session
            .run("get", &["--human".into(), "--page".into(), "basic".into()])
            .await;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!result.stdout.iter().any(|byte| [0x1b, 0x07].contains(byte)));
        let path = session.directory.path().join("empty-session.json");
        let password = session.private("empty-password", b"\n");
        let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_ht813-console"))
            .kill_on_drop(true)
            .args(["login", "--human", "--origin", &fixture.origin, "--session"])
            .arg(&path)
            .arg("--password-file")
            .arg(&password)
            .output()
            .await
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("パスワードが空"));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn cli_selects_human_output_in_a_terminal_and_json_when_requested() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        for (json, plain) in [(false, true), (true, true), (false, false)] {
            let mut command = tokio::process::Command::new("script");
            command
                .kill_on_drop(true)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .env("TERM", "xterm-256color")
                .env("NO_COLOR", "1");
            if !plain {
                command.env_remove("NO_COLOR");
            }
            let arguments = vec![
                "get".to_owned(),
                "--session".into(),
                session.path.display().to_string(),
                "--no-progress".into(),
            ];
            #[cfg(target_os = "macos")]
            {
                command
                    .args(["-q", "/dev/null", env!("CARGO_BIN_EXE_ht813-console")])
                    .args(&arguments);
                if json {
                    command.arg("--json");
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let invocation = std::iter::once(env!("CARGO_BIN_EXE_ht813-console").to_owned())
                    .chain(arguments)
                    .chain(json.then_some("--json".to_owned()))
                    .map(|value| format!("'{}'", value.replace('\'', "'\"'\"'")))
                    .collect::<Vec<_>>()
                    .join(" ");
                command.args(["--quiet", "--return", "--command", &invocation, "/dev/null"]);
            }
            let mut child = command.spawn().unwrap();
            let input = child.stdin.take();
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(15), child.wait_with_output())
                    .await
                    .unwrap()
                    .unwrap();
            drop(input);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            if json {
                let snapshot: Value =
                    serde_json::from_slice(&result.stdout).unwrap_or_else(|error| {
                        panic!("{error}: {:?}", String::from_utf8_lossy(&result.stdout))
                    });
                assert_eq!(snapshot["page"], "status");
            } else {
                assert!(String::from_utf8_lossy(&result.stdout).contains("状態を取得しました"));
                assert!(!String::from_utf8_lossy(&result.stdout).contains("\"revision\""));
                if plain {
                    assert!(!result.stdout.contains(&0x1b));
                } else {
                    let styled = String::from_utf8_lossy(&result.stdout);
                    assert!(styled.contains("\u{1b}[1m"));
                    assert!(
                        !styled
                            .replace("\u{1b}[1m", "")
                            .replace("\u{1b}[0m", "")
                            .contains('\u{1b}')
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn cli_human_failures_preserve_uncertainty_and_explain_connection_errors() {
        let fixture = Fixture::start().await;
        let session = CliSession::start(&fixture).await;
        let (plan, reviewed) = session
            .plan(
                "[values]\nserver = [\"sip.human-failure.invalid\"]\n",
                "human-failure",
            )
            .await;
        assert!(reviewed.status.success());
        fixture.device.lock().unwrap().server = Some("sip.external-fixture.invalid".into());
        let result = session
            .run(
                "apply",
                &[
                    "--human".into(),
                    "--plan".into(),
                    plan.display().to_string(),
                ],
            )
            .await;
        assert_eq!(result.status.code(), Some(3));
        let explanation = String::from_utf8_lossy(&result.stderr);
        assert!(explanation.contains("\n  現在の設定"), "{explanation:?}");
        assert!(!explanation.contains("\\n"));
        assert!(String::from_utf8_lossy(&result.stdout).contains("reconcile"));
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
        assert!(!session.apply(&plan).await.status.success());
        let mut state: Value =
            serde_json::from_slice(&std::fs::read(&session.path).unwrap()).unwrap();
        state["origin"] = json!("http://127.0.0.1:0");
        std::fs::write(&session.path, serde_json::to_vec(&state).unwrap()).unwrap();
        let result = session.run("get", &["--human".into()]).await;
        assert_eq!(result.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("管理画面との通信を完了できません")
        );
    }

    async fn native_get(State(state): State<Shared>, headers: HeaderMap) -> impl IntoResponse {
        let mut device = state.lock().unwrap();
        let expected = format!("native={}", device.generation);
        if headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) != Some(&expected) {
            return (StatusCode::OK, LOGIN.to_owned());
        }
        device.token += 1;
        (
            StatusCode::OK,
            SETTINGS
                .replace("csrf-fixture", &format!("csrf-{}", device.token))
                .replace(
                    "sip.example.invalid",
                    device.server.as_deref().unwrap_or("sip.example.invalid"),
                )
                .replace(
                    "fixture-existing",
                    device.secret.as_deref().unwrap_or("fixture-existing"),
                )
                .replace(
                    "name=\"server\"",
                    if device.replace_parameter_after_write && device.writes > 0 {
                        "name=\"P747\""
                    } else {
                        "name=\"server\""
                    },
                ),
        )
    }

    async fn native_status(State(state): State<Shared>, headers: HeaderMap) -> impl IntoResponse {
        let expected = format!("native={}", state.lock().unwrap().generation);
        if headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) != Some(&expected) {
            return LOGIN.to_owned();
        }
        "<table><tr><td>Program --</td><td>1.0.17.3</td></tr><tr><td>FXS</td><td>On Hook / Not Registered</td></tr></table>".to_owned()
    }

    async fn native_login(
        State(state): State<Shared>,
        Form(values): Form<std::collections::BTreeMap<String, String>>,
    ) -> impl IntoResponse {
        let mut device = state.lock().unwrap();
        device.logins += 1;
        if values.get("username").map(String::as_str) != Some("admin")
            || values.get("P2").map(String::as_str) != Some("c2VjcmV0")
            || values.get("csrf_token").map(String::as_str) != Some("csrf-fixture")
            || values.get("gnkey").map(String::as_str) != Some("key-fixture")
        {
            return (StatusCode::UNAUTHORIZED, HeaderMap::new(), LOGIN.to_owned());
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::SET_COOKIE,
            format!("native={}; Path=/; HttpOnly", device.generation)
                .parse()
                .unwrap(),
        );
        (StatusCode::OK, headers, "authenticated".to_owned())
    }

    async fn native_write(
        State(state): State<Shared>,
        Form(values): Form<std::collections::BTreeMap<String, String>>,
    ) -> impl IntoResponse {
        let mut device = state.lock().unwrap();
        if values.get("csrf_token") != Some(&format!("csrf-{}", device.token))
            || values.get("gnkey").map(String::as_str) != Some("key-fixture")
            || values.get("future_flag").map(String::as_str) != Some("preserve-me")
            || !values.contains_key("secret")
        {
            return StatusCode::BAD_REQUEST;
        }
        device.writes += 1;
        if !device.ignore_server {
            device.server = values.get("server").cloned();
        }
        device.secret = values.get("secret").cloned();
        if device.fail_after_write {
            StatusCode::INTERNAL_SERVER_ERROR
        } else {
            StatusCode::OK
        }
    }

    struct Fixture {
        origin: String,
        client: reqwest::Client,
        device: Shared,
        tasks: Vec<JoinHandle<()>>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            for task in &self.tasks {
                task.abort();
            }
        }
    }
    impl Fixture {
        async fn start() -> Self {
            let device = Shared::default();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let native_origin = format!("http://{}", listener.local_addr().unwrap());
            let native = Router::new()
                .route("/cgi-bin/index", get(native_status))
                .route("/cgi-bin/dologin", post(native_login))
                .route("/cgi-bin/update", post(native_write))
                .fallback(get(native_get))
                .with_state(device.clone());
            let native_task = tokio::spawn(async move {
                axum::serve(listener, native).await.unwrap();
            });
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let origin = format!("http://{address}");
            let app = router(Config {
                listen: address,
                public_origin: origin.parse().unwrap(),
                device_origin: native_origin.parse().unwrap(),
                assets: std::path::PathBuf::from("console/dist"),
            })
            .unwrap();
            let app_task = tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });
            Self {
                origin,
                client: reqwest::Client::builder()
                    .cookie_store(true)
                    .build()
                    .unwrap(),
                device,
                tasks: vec![native_task, app_task],
            }
        }
        fn path(&self, path: &str) -> String {
            format!("{}/api/console/{path}", self.origin)
        }
        async fn login(&self) -> reqwest::Response {
            self.client
                .post(self.path("session"))
                .header("Origin", &self.origin)
                .json(&json!({"username":"admin","password":"secret"}))
                .send()
                .await
                .unwrap()
        }
        async fn basic(&self) -> Value {
            let response = self
                .client
                .get(self.path("pages/basic"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            response.json().await.unwrap()
        }
        async fn write(&self, revision: &Value) -> reqwest::Response {
            self.client.post(self.path("pages/basic")).header("Origin", &self.origin)
                .json(&json!({"revision":revision,"mode":"save","values":{"field-0":["sip.changed.invalid"]}}))
                .send().await.unwrap()
        }
    }

    #[test]
    fn console_configuration_refuses_public_listeners_and_ambiguous_origins() {
        let mut config = Config {
            listen: "127.0.0.1:8790".parse().unwrap(),
            public_origin: Url::parse("http://127.0.0.1:8790").unwrap(),
            device_origin: Url::parse("http://192.0.2.13").unwrap(),
            assets: "console/dist".into(),
        };
        assert!(config.validate().is_ok());
        config.listen = "0.0.0.0:8790".parse().unwrap();
        assert!(config.validate().is_err());
        config.listen = "127.0.0.1:8790".parse().unwrap();
        config.device_origin.set_path("/other");
        assert!(config.validate().is_err());
        config.device_origin.set_path("/");
        config.public_origin = Url::parse("http://console.example.invalid").unwrap();
        assert!(config.validate().is_err());
        config.public_origin = Url::parse("https://console.example.invalid").unwrap();
        assert!(config.validate().is_ok());
    }

    #[tokio::test]
    async fn remote_console_requires_identity_and_device_authentication_without_a_user_list() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let assets = tempfile::tempdir().unwrap();
        std::fs::write(assets.path().join("index.html"), "<title>Fixture</title>").unwrap();
        let app = router(Config {
            listen: "127.0.0.1:8790".parse().unwrap(),
            public_origin: "https://console.example.invalid".parse().unwrap(),
            device_origin: "http://127.0.0.1:9".parse().unwrap(),
            assets: assets.path().into(),
        })
        .unwrap();
        let readiness = Request::builder()
            .uri("/")
            .header("tailscale-user-login", "homephone-readiness")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(readiness).await.unwrap().status(),
            StatusCode::OK
        );
        let unidentified = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert_eq!(
            app.clone().oneshot(unidentified).await.unwrap().status(),
            StatusCode::OK
        );
        for (name, value, expected) in [
            ("tailscale-user-login", None, StatusCode::FORBIDDEN),
            ("tailscale-user-login", Some(""), StatusCode::FORBIDDEN),
            ("tailscale-user-login", Some(" "), StatusCode::FORBIDDEN),
            (
                "tailscale_user_login",
                Some("person@example.invalid"),
                StatusCode::FORBIDDEN,
            ),
            (
                "tailscale-user-login",
                Some("person@example.invalid"),
                StatusCode::OK,
            ),
            (
                "tailscale-user-login",
                Some("another@example.invalid"),
                StatusCode::OK,
            ),
        ] {
            let mut request = Request::builder().uri("/api/console/session");
            if let Some(value) = value {
                request = request.header(name, value);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                let body = axum::body::to_bytes(response.into_body(), 4096)
                    .await
                    .unwrap();
                let session: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(session["authenticated"], false);
            }
        }
        for (method, path, origin, expected) in [
            (
                "GET",
                "/api/console/pages/basic",
                None,
                StatusCode::UNAUTHORIZED,
            ),
            (
                "POST",
                "/api/console/session",
                Some("https://foreign.invalid"),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("tailscale-user-login", "person@example.invalid");
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            assert_eq!(
                app.clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                expected
            );
        }
        let duplicate = Request::builder()
            .uri("/api/console/session")
            .header("tailscale-user-login", "person@example.invalid")
            .header("tailscale-user-login", "another@example.invalid")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(duplicate).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn browser_authentication_hides_native_credentials_and_revokes_logout() {
        let fixture = Fixture::start().await;
        let response = fixture.login().await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get("Set-Cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
        let body = response.text().await.unwrap();
        assert!(
            !body.contains("secret")
                && !body.contains("csrf-fixture")
                && !body.contains("key-fixture")
        );
        assert_eq!(fixture.device.lock().unwrap().logins, 1);
        let snapshot = fixture.basic().await;
        assert_eq!(
            snapshot["settings"][0]["field"]["values"],
            json!(["sip.example.invalid"])
        );
        assert!(!snapshot.to_string().contains("fixture-existing"));
        let logout = fixture
            .client
            .delete(fixture.path("session"))
            .header("Origin", &fixture.origin)
            .send()
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            fixture
                .client
                .get(fixture.path("pages/basic"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn cross_origin_and_unknown_fields_do_not_reach_native_authentication() {
        let fixture = Fixture::start().await;
        for origin in [None, Some("https://attacker.invalid")] {
            let mut request = fixture.client.post(fixture.path("session"));
            if let Some(origin) = origin {
                request = request.header("Origin", origin);
            }
            let response = request
                .json(&json!({"username":"admin","password":"secret"}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let response = fixture
            .client
            .post(fixture.path("session"))
            .header("Origin", &fixture.origin)
            .json(&json!({"username":"admin","password":"secret","url":"http://attacker.invalid"}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_client_error());
        assert_eq!(fixture.device.lock().unwrap().logins, 0);
    }

    #[tokio::test]
    async fn native_read_authentication_recovers_without_invalidating_an_unchanged_draft() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let before = fixture.basic().await;
        fixture.device.lock().unwrap().generation += 1;
        let after = fixture.basic().await;
        assert_eq!(after["revision"], before["revision"]);
        assert_eq!(after["native_session_recovered"], true);
        assert_eq!(fixture.device.lock().unwrap().logins, 2);
        assert_eq!(fixture.device.lock().unwrap().writes, 0);
    }

    #[tokio::test]
    async fn reviewed_writes_use_fresh_tokens_and_cannot_replay_the_old_revision() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let before = fixture.basic().await;
        let response = fixture.write(&before["revision"]).await;
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: Value = response.json().await.unwrap();
        assert_eq!(receipt["outcome"], "confirmed");
        assert_eq!(
            receipt["snapshot"]["settings"][0]["field"]["values"],
            json!(["sip.changed.invalid"])
        );
        let replay = fixture.write(&before["revision"]).await;
        assert_eq!(replay.status(), StatusCode::CONFLICT);
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
    }

    #[tokio::test]
    async fn a_failed_native_response_is_reconciled_without_automatic_write_retry() {
        let fixture = Fixture::start().await;
        assert_eq!(fixture.login().await.status(), StatusCode::OK);
        let before = fixture.basic().await;
        fixture.device.lock().unwrap().fail_after_write = true;
        let response = fixture.write(&before["revision"]).await;
        assert_eq!(response.status(), StatusCode::OK);
        let receipt: Value = response.json().await.unwrap();
        assert_eq!(receipt["outcome"], "uncertain");
        assert!(receipt["snapshot"].is_object());
        assert_eq!(fixture.device.lock().unwrap().writes, 1);
        assert_eq!(
            fixture.write(&before["revision"]).await.status(),
            StatusCode::CONFLICT
        );
    }
}

const LOGIN: &str = r#"
<form name="loginForm" method="post" action="dologin" onsubmit="return checkForm()">
<input type="hidden" name="csrf_token" value="csrf-fixture">
<input type="hidden" name="gnkey" value="key-fixture">
<label>Username<input name="username" value="admin" maxlength="30"></label>
<label>Password<input name="P2" type="password" id="input-password" maxlength="30"></label>
<input type="submit" name="Login" value="Login">
</form>
<script>
function checkForm() {
  var input_pwd = document.getElementById('input-password');
  input_pwd.value = btoa(input_pwd.value);
  return true;
}
</script>
"#;

const SETTINGS: &str = r#"
<form method="post" action="update">
<input type="hidden" name="csrf_token" value="csrf-fixture">
<input type="hidden" name="gnkey" value="key-fixture">
<input type="hidden" name="future_flag" value="preserve-me">
<table>
<tr><td>Primary SIP Server:</td><td><input name="server" value="sip.example.invalid" maxlength="128"></td></tr>
<tr><td>Account Active:</td><td><label><input name="enabled" type="radio" value="1" checked>Yes</label><label><input name="enabled" type="radio" value="0">No</label></td></tr>
<tr><td>Preferred Vocoder:</td><td><select name="codec"><option value="0" selected>PCMU</option><option value="8">PCMA</option></select></td></tr>
<tr><td>Dial Plan:</td><td><textarea name="plan"> { x+ }
</textarea></td></tr>
<tr><td>Call Waiting:</td><td><input name="waiting" type="checkbox" value="1" checked></td></tr>
<tr><td>Disabled setting:</td><td><input name="inactive" value="9" disabled></td></tr>
<tr><td>Authenticate Password:</td><td><input name="secret" type="password" value="fixture-existing"></td></tr>
</table>
<input name="update" type="submit" value="Update">
<input name="apply" type="submit" value="Apply">
</form>
"#;

fn document(path: &str) -> Url {
    Url::parse(&format!("http://device.example.invalid/cgi-bin/{path}")).unwrap()
}

fn settings() -> NativeForm {
    NativeForm::parse(SETTINGS, &document("config_a2")).unwrap()
}

#[test]
fn external_scripts_cannot_silently_qualify_as_plain_settings() {
    for source in [
        "../scripts/unknown.js",
        "https://other.example.invalid/unknown.js",
    ] {
        let html = format!("{SETTINGS}<script src='{source}'></script>");
        let form = NativeForm::parse(&html, &document("config_a2")).unwrap();
        assert!(!form.supports_mode(Mode::Save));
        assert!(form.settings_values(&BTreeMap::new(), Mode::Save).is_err());
    }
}

#[test]
fn native_session_tokens_rotate_without_changing_a_reviewed_setting() {
    let html = SETTINGS.replace("csrf_token", "session_token");
    let form = NativeForm::parse(&html, &document("config_a2")).unwrap();
    let values = form.settings_values(&BTreeMap::new(), Mode::Save).unwrap();
    assert!(values.contains(&("session_token".into(), "csrf-fixture".into())));
    let rotated = html.replace("csrf-fixture", "rotated-session-fixture");
    assert!(form.same_settings(&NativeForm::parse(&rotated, &document("config_a2")).unwrap()));
    for invalid in [
        html.replace(
            "</form>",
            "<input type='hidden' name='csrf_token' value='ambiguous'></form>",
        ),
        html.replace("name=\"session_token\"", "name=\"session_token\" disabled"),
        html.replace(
            "type=\"hidden\" name=\"session_token\"",
            "type=\"text\" name=\"session_token\"",
        ),
        html.replace("value=\"csrf-fixture\"", "value=\"\""),
    ] {
        assert!(NativeForm::parse(&invalid, &document("config_a2")).is_err());
    }
}

#[test]
fn login_preserves_tokens_and_the_native_latin1_base64_encoding() {
    let form = NativeForm::parse(LOGIN, &document("login")).unwrap();
    assert_eq!(form.endpoint(), &document("dologin"));
    assert_eq!(
        form.login_values("admin", "caf\u{e9}").unwrap(),
        [
            ("csrf_token", "csrf-fixture"),
            ("gnkey", "key-fixture"),
            ("username", "admin"),
            ("P2", "Y2Fm6Q=="),
            ("Login", "Login"),
        ]
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
    );
    assert_eq!(
        form.login_values("admin", "密"),
        Err(NativeError::InvalidCredentials)
    );
    assert_eq!(
        form.login_values("admin", &"x".repeat(31)),
        Err(NativeError::InvalidCredentials)
    );
}

#[test]
fn form_parsing_groups_choices_and_redacts_password_values() {
    let form = settings();
    let fields = form.fields();
    assert_eq!(fields.len(), 7);
    assert_eq!(fields[0].label, "SIPサーバー");
    assert_eq!(fields[1].kind, FieldKind::Radio);
    assert_eq!(fields[1].choices.len(), 2);
    assert_eq!(fields[1].values, ["1"]);
    assert_eq!(fields[2].values, ["0"]);
    assert_eq!(fields[3].values, [" { x+ }\n"]);
    assert_eq!(fields[4].kind, FieldKind::Checkbox);
    assert!(fields[5].disabled);
    assert_eq!(fields[6].kind, FieldKind::Password);
    assert!(fields[6].values.is_empty());
}

#[test]
fn settings_preserve_successful_controls_and_the_clicked_submitter() {
    let form = settings();
    let changes = BTreeMap::from([
        (
            form.fields()[0].id.clone(),
            vec!["pbx.example.invalid".into()],
        ),
        (form.fields()[4].id.clone(), vec![]),
    ]);
    let values = form.settings_values(&changes, Mode::SaveAndApply).unwrap();
    assert!(values.contains(&("future_flag".into(), "preserve-me".into())));
    assert!(values.contains(&("server".into(), "pbx.example.invalid".into())));
    assert!(values.contains(&("secret".into(), "fixture-existing".into())));
    assert!(values.contains(&("apply".into(), "Apply".into())));
    assert!(
        !values
            .iter()
            .any(|(name, _)| ["inactive", "waiting", "update"].contains(&name.as_str()))
    );
    let saved = form.settings_values(&changes, Mode::Save).unwrap();
    assert!(saved.contains(&("update".into(), "Update".into())));
    assert!(!saved.iter().any(|(name, _)| name == "apply"));
}

#[test]
fn untrusted_field_changes_cannot_replace_tokens_or_escape_choices() {
    let form = settings();
    for changes in [
        BTreeMap::from([("csrf_token".into(), vec!["forged".into()])]),
        BTreeMap::from([(form.fields()[1].id.clone(), vec!["unlisted".into()])]),
        BTreeMap::from([(form.fields()[5].id.clone(), vec!["changed".into()])]),
        BTreeMap::from([(form.fields()[0].id.clone(), vec!["x".repeat(129)])]),
    ] {
        assert_eq!(
            form.settings_values(&changes, Mode::Save),
            Err(NativeError::InvalidChange)
        );
    }
}

#[test]
fn token_rotation_does_not_hide_changed_controls_or_reordered_fields() {
    let form = settings();
    let rotated = SETTINGS
        .replace("csrf-fixture", "fresh-csrf")
        .replace("key-fixture", "fresh-key");
    assert!(form.same_settings(&NativeForm::parse(&rotated, &document("config_a2")).unwrap()));
    let changed = SETTINGS.replace("sip.example.invalid", "other.example.invalid");
    assert!(!form.same_settings(&NativeForm::parse(&changed, &document("config_a2")).unwrap()));
    let changed_secret = SETTINGS.replace("fixture-existing", "fixture-replaced");
    assert!(
        !form.same_settings(&NativeForm::parse(&changed_secret, &document("config_a2")).unwrap())
    );
}

#[test]
fn external_endpoints_and_ambiguous_native_tokens_are_refused() {
    for action in [
        "https://external.example.invalid/collect",
        "/outside-gateway",
        "http://user:pass@device.example.invalid/cgi-bin/update",
    ] {
        let html = SETTINGS.replace("action=\"update\"", &format!("action=\"{action}\""));
        assert!(matches!(
            NativeForm::parse(&html, &document("config_a2")),
            Err(NativeError::InvalidEndpoint)
        ));
    }
    let duplicated = SETTINGS.replace(
        "<form method=\"post\" action=\"update\">",
        "<form method=\"post\" action=\"update\"><input type=\"hidden\" name=\"csrf_token\" value=\"duplicate\">",
    );
    assert!(matches!(
        NativeForm::parse(&duplicated, &document("config_a2")),
        Err(NativeError::InvalidToken)
    ));
}

#[test]
fn login_refuses_ambiguous_or_disabled_credentials_and_different_handlers() {
    for html in [
        LOGIN.replace(
            "</form>",
            "<input name=\"username\" value=\"other\"></form>",
        ),
        LOGIN.replace("return checkForm()", "return differentHandler()"),
        LOGIN.replace("type=\"password\" id=", "type=\"password\" disabled id="),
    ] {
        assert!(matches!(
            NativeForm::parse(&html, &document("login")),
            Err(NativeError::UnsupportedForm)
        ));
    }
}

#[test]
fn uppercase_submit_types_preserve_native_browser_behavior() {
    let html = SETTINGS.replace("type=\"submit\"", "type=\"SUBMIT\"");
    let form = NativeForm::parse(&html, &document("config_a2")).unwrap();
    assert!(form.supports_mode(Mode::Save));
    assert!(form.supports_mode(Mode::SaveAndApply));
}

#[test]
fn disabled_option_groups_are_not_submittable() {
    let html = SETTINGS.replace(
        "<option value=\"8\">PCMA</option>",
        "<optgroup label=\"Unavailable\" disabled><option value=\"8\">PCMA</option></optgroup>",
    );
    let form = NativeForm::parse(&html, &document("config_a2")).unwrap();
    assert!(form.fields()[2].choices[1].disabled);
    assert_eq!(
        form.settings_values(
            &BTreeMap::from([(form.fields()[2].id.clone(), vec!["8".into()])]),
            Mode::Save
        ),
        Err(NativeError::InvalidChange)
    );
}

#[test]
fn unchanged_required_fields_still_block_an_invalid_submission() {
    let html = SETTINGS.replace(
        "value=\"sip.example.invalid\" maxlength=",
        "value=\"\" required maxlength=",
    );
    let form = NativeForm::parse(&html, &document("config_a2")).unwrap();
    assert!(form.fields()[0].required);
    assert_eq!(
        form.settings_values(
            &BTreeMap::from([(form.fields()[4].id.clone(), vec![])]),
            Mode::Save
        ),
        Err(NativeError::InvalidChange)
    );
}

#[test]
fn oversized_forms_and_unknown_script_transforms_are_refused() {
    let oversized = " ".repeat(phone_server::console::native::MAX_HTML_BYTES + 1);
    assert!(matches!(
        NativeForm::parse(&oversized, &document("config_a2")),
        Err(NativeError::TooLarge)
    ));
    let html = format!("{SETTINGS}<script>transformUnknownFields();</script>");
    let form = NativeForm::parse(&html, &document("config_a2")).unwrap();
    assert!(!form.supports_mode(Mode::Save));
    assert_eq!(
        form.settings_values(&BTreeMap::new(), Mode::Save),
        Err(NativeError::UnsupportedForm)
    );
}
