use super::{NativeError, selector};
use scraper::{ElementRef, Html};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

pub const RESOURCE_PATHS: [&str; 3] = [
    "/scripts/jquery-min.js",
    "/scripts/jquery.validate.min.js",
    "/scripts/common.js",
];
const ASSETS: [(&str, &str); 3] = [
    (
        RESOURCE_PATHS[0],
        "ff1523fb7389539c84c65aba19260648793bb4f5e29329d2ee8804bc37a3fe6e",
    ),
    (
        RESOURCE_PATHS[1],
        "a931d758e10b5f646f42e4b1100ee31b7ce4cdf5a86d59133424b65c8802788b",
    ),
    (
        RESOURCE_PATHS[2],
        "28a2382cc60477a2fb0a85b6f1f07a86041f07ef6687aaf6442f5662f5fba6ab",
    ),
];

#[derive(Default)]
pub struct Resources {
    hashes: BTreeMap<String, String>,
}
impl Resources {
    pub fn record(&mut self, path: &str, bytes: &[u8]) {
        self.hashes.insert(path.to_owned(), digest(bytes));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Script {
    Fxs,
    Fxo,
    OptionsFxs,
    OptionsFxo,
    Advanced,
    Basic,
    Reset,
    Initial,
}
const SCRIPTS: [(Script, &str); 9] = [
    (
        Script::Fxs,
        "1fe22f4536a21263d03281ddb817489c36dfea79438933def0e236eff697cd88",
    ),
    (
        Script::Fxo,
        "ae7d813a7b45697e010f51d5f09fa5bd73cead3feb2dae6bca0d3d20052798e9",
    ),
    (
        Script::OptionsFxs,
        "357e22e2127105f497b87516834b03103a7191ecdcfe1325d276d2c38a1e54b1",
    ),
    (
        Script::OptionsFxo,
        "8794a4f600cf43bf95a8011cd7aede087ac6c947b4ddd09f979f6456f069feea",
    ),
    (
        Script::Advanced,
        "43d7f484f5da32e65fa072fd2fbaa214bb8ac793a590e8c57249a59f4611ff84",
    ),
    (
        Script::Basic,
        "6bdbad10e0da25e43083b4f1af3b64dd332a9747885a6e8ac48a834d2951820d",
    ),
    (
        Script::Reset,
        "cd73f4e1705752c686fb1b36eb427b58ca01538bd7b9e453fdaf607775899583",
    ),
    (
        Script::Initial,
        "937a11a63810a69398536d25786f5e3a95021beae92edfc0d007b36386663fee",
    ),
    (
        Script::Initial,
        "133c60c3fe674edc1da18abbd5a42cf1a7e851d629dac12076400403e89a3cf7",
    ),
];
struct Manifest<'a> {
    assets: &'a [(&'a str, &'a str)],
    scripts: &'a [(Script, &'a str)],
}
const MANIFEST: Manifest<'static> = Manifest {
    assets: &ASSETS,
    scripts: &SCRIPTS,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Basic,
    Advanced,
    Fxs,
    Fxo,
}
impl Page {
    fn from_url(url: &Url) -> Option<Self> {
        match url.path() {
            "/cgi-bin/config2" => Some(Self::Basic),
            "/cgi-bin/config" => Some(Self::Advanced),
            "/cgi-bin/config_a1" => Some(Self::Fxs),
            "/cgi-bin/config_a2" => Some(Self::Fxo),
            _ => None,
        }
    }
    fn scripts(self) -> &'static [Script] {
        match self {
            Self::Basic => &[Script::Basic, Script::Reset, Script::Initial],
            Self::Advanced => &[Script::Advanced, Script::Initial],
            Self::Fxs => &[Script::Fxs, Script::OptionsFxs, Script::Initial],
            Self::Fxo => &[Script::Fxo, Script::OptionsFxo, Script::Initial],
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Plan {
    page: Page,
    peer: Option<u16>,
    generated: Vec<(String, String)>,
}
impl Plan {
    pub(super) fn inspect(html: &Html, url: &Url, resources: &Resources) -> Option<Self> {
        inspect(html, url, resources, &MANIFEST)
    }
    pub(super) fn fragments(&self) -> impl Iterator<Item = (&str, &str)> {
        self.generated
            .iter()
            .map(|(hash, markup)| (hash.as_str(), markup.as_str()))
    }
    pub(super) fn validate(&self, values: &[(String, String)]) -> Result<(), NativeError> {
        let value = |name: &str| -> Result<&str, NativeError> {
            let mut matches = values.iter().filter(|(key, _)| key == name);
            let found = matches.next().ok_or(NativeError::InvalidChange)?;
            if matches.next().is_some() {
                return Err(NativeError::InvalidChange);
            }
            Ok(found.1.as_str())
        };
        if let Some(peer) = self.peer {
            let local = value(if self.page == Page::Fxs {
                "P40"
            } else {
                "P740"
            })?;
            let local = decimal(local)
                .and_then(|v| u16::try_from(v).ok())
                .filter(|v| *v > 0)
                .ok_or(NativeError::InvalidChange)?;
            if local == peer {
                return Err(NativeError::InvalidChange);
            }
        }
        if self.page == Page::Advanced {
            let minimum = value("P22293")?;
            let maximum = value("P22294")?;
            if minimum != "99" && minimum > maximum {
                return Err(NativeError::InvalidChange);
            }
        }
        Ok(())
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn resource_urls(html: &str, document: &Url) -> Vec<Url> {
    if html.len() > super::MAX_HTML_BYTES || Page::from_url(document).is_none() {
        return vec![];
    }
    let parsed = Html::parse_document(html);
    resource_urls_with(&parsed, document).unwrap_or_default()
}
fn resource_urls_with(html: &Html, document: &Url) -> Option<Vec<Url>> {
    let mut paths = BTreeMap::new();
    for node in html.select(&selector("script").ok()?) {
        if let Some(source) = node.value().attr("src") {
            let url = document.join(source).ok()?;
            if url.origin() != document.origin()
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
                || !RESOURCE_PATHS.contains(&url.path())
                || !node.text().collect::<String>().trim().is_empty()
                || paths.insert(url.path().to_owned(), url).is_some()
            {
                return None;
            }
        }
    }
    if paths.len() != RESOURCE_PATHS.len() {
        return None;
    }
    Some(paths.into_values().collect())
}

pub(super) fn editable_password(node: ElementRef<'_>) -> bool {
    node.value().name() == "input"
        && node
            .value()
            .attr("type")
            .is_some_and(|kind| kind.eq_ignore_ascii_case("password"))
        && matches!(
            node.value().attr("onfocus"),
            Some(
                "javascript:this.removeAttribute('readonly');this.select();"
                    | "javascript:this.removeAttribute('readonly');this.style.borderColor='initial';this.select();"
            )
        )
}

fn inspect(html: &Html, url: &Url, resources: &Resources, manifest: &Manifest<'_>) -> Option<Plan> {
    let page = Page::from_url(url)?;
    if html
        .select(&selector("base, [form], [dirname]").ok()?)
        .next()
        .is_some()
    {
        return None;
    }
    let urls = resource_urls_with(html, url)?;
    if manifest.assets.len() != urls.len()
        || manifest
            .assets
            .iter()
            .any(|(path, hash)| resources.hashes.get(*path).map(String::as_str) != Some(*hash))
    {
        return None;
    }
    let handler = if matches!(page, Page::Fxs | Page::Fxo) {
        "return checkSIPPort()"
    } else {
        "return checkPassword()"
    };
    let forms_selector = selector("form[onsubmit]").ok()?;
    let mut forms = html.select(&forms_selector);
    let form = forms.next()?;
    if forms.next().is_some()
        || form.value().attr("onsubmit") != Some(handler)
        || form.value().attr("action") != Some("update")
        || !form.value().attr("method")?.eq_ignore_ascii_case("post")
        || !bindings(html, form, page)
    {
        return None;
    }
    let mut plan = Plan {
        page,
        peer: None,
        generated: vec![],
    };
    let mut seen = BTreeSet::new();
    for node in html.select(&selector("script").ok()?) {
        if node.value().attrs().any(|(name, _)| {
            name.starts_with("on") || matches!(name, "async" | "defer" | "nomodule")
        }) {
            return None;
        }
        if !matches!(
            node.value().attr("type"),
            None | Some("text/javascript" | "application/javascript")
        ) {
            return None;
        }
        if node.value().attr("src").is_some() {
            continue;
        }
        let source = node.text().collect::<String>();
        if source.trim().is_empty() {
            continue;
        }
        let (normalized, data) = normalize(&source)?;
        let hash = digest(normalized.as_bytes());
        let kind = manifest.scripts.iter().find(|(_, known)| *known == hash)?.0;
        if !page.scripts().contains(&kind) || !seen.insert(kind) {
            return None;
        }
        match kind {
            Script::Fxs | Script::Fxo => {
                plan.peer = Some(data.peer?);
            }
            Script::OptionsFxs | Script::OptionsFxo => {
                let parameter = if kind == Script::OptionsFxs {
                    "P2399"
                } else {
                    "P2499"
                };
                if !node.ancestors().any(|ancestor| ancestor.id() == form.id())
                    || form
                        .select(&selector("[name]").ok()?)
                        .any(|control| control.value().attr("name") == Some(parameter))
                {
                    return None;
                }
                let value = data.generated?;
                let markup = format!(
                    "<label>SIP OPTIONS Keep Alive Max Lost<input name='{parameter}' value='{value}' maxlength='2'></label>"
                );
                plan.generated.push((digest(source.as_bytes()), markup));
            }
            Script::Reset if data.role.as_deref()? == "admin" && !data.reset_disabled? => {
                plan.generated.push((
                    digest(source.as_bytes()),
                    "<input type='hidden' name='reset_type' value='1'>".into(),
                ));
            }
            _ => {}
        }
    }
    if seen != page.scripts().iter().copied().collect() {
        return None;
    }
    for node in html.select(&selector("*").ok()?) {
        for (name, handler) in node
            .value()
            .attrs()
            .filter(|(name, _)| name.starts_with("on"))
        {
            let element = node.value();
            let kind = element.attr("type").unwrap_or("");
            let valid = match name {
                "onsubmit" if element.name() == "form" => {
                    handler
                        == if matches!(page, Page::Fxs | Page::Fxo) {
                            "return checkSIPPort()"
                        } else {
                            "return checkPassword()"
                        }
                }
                "onfocus" => editable_password(node),
                "onclick" if element.name() == "input" && kind.eq_ignore_ascii_case("button") => {
                    true
                }
                "onclick"
                    if page == Page::Fxs
                        && element.attr("name") == Some("P130")
                        && kind.eq_ignore_ascii_case("radio") =>
                {
                    matches!(
                        (element.attr("value"), handler),
                        (Some("0" | "1"), "portChange(5060);") | (Some("2"), "portChange(5061);")
                    )
                }
                _ => false,
            };
            if !valid {
                return None;
            }
        }
    }
    Some(plan)
}

fn bindings(html: &Html, form: ElementRef<'_>, page: Page) -> bool {
    let unique = |name: &str, kind: &str| {
        let Ok(selector) = selector("input, select") else {
            return false;
        };
        let mut matches = form
            .select(&selector)
            .filter(|node| node.value().attr("name") == Some(name));
        let Some(node) = matches.next() else {
            return false;
        };
        matches.next().is_none()
            && if kind == "select" {
                node.value().name() == "select"
            } else {
                node.value().name() == "input"
                    && node
                        .value()
                        .attr("type")
                        .unwrap_or("text")
                        .eq_ignore_ascii_case(kind)
            }
    };
    let password_pair = |id: &str, confirmation: &str, parameter: &str| {
        let Ok(selector) = selector("[id]") else {
            return false;
        };
        [(id, Some(parameter)), (confirmation, None)]
            .into_iter()
            .all(|(id, name)| {
                let mut matches = html
                    .select(&selector)
                    .filter(|node| node.value().attr("id") == Some(id));
                let Some(node) = matches.next() else {
                    return false;
                };
                matches.next().is_none()
                    && node.ancestors().any(|ancestor| ancestor.id() == form.id())
                    && node.value().attr("name") == name
                    && editable_password(node)
            })
    };
    match page {
        Page::Fxs => unique("P40", "text") && unique("P34", "password"),
        Page::Fxo => unique("P740", "text") && unique("P734", "password"),
        Page::Advanced => {
            password_pair("pass1", "pass2", "P2")
                && unique("P22293", "select")
                && unique("P22294", "select")
        }
        Page::Basic => {
            form.value().attr("name") == Some("basic_form")
                && password_pair("pass1", "pass2", "P196")
                && password_pair("passViewer1", "passViewer2", "P28113")
        }
    }
}

#[derive(Default)]
struct Data {
    peer: Option<u16>,
    generated: Option<u8>,
    role: Option<String>,
    reset_disabled: Option<bool>,
}
fn decimal(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}
fn slot(
    source: &mut String,
    prefix: &str,
    suffix: &str,
    replacement: &str,
) -> Option<Option<String>> {
    let Some(start) = source.find(prefix).map(|p| p + prefix.len()) else {
        return Some(None);
    };
    let end = start + source.get(start..)?.find(suffix)?;
    let value = source.get(start..end)?.to_owned();
    if source.get(end + suffix.len()..)?.contains(prefix) {
        return None;
    }
    source.replace_range(start..end, replacement);
    Some(Some(value))
}
fn normalize(source: &str) -> Option<(String, Data)> {
    let mut source = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut data = Data::default();
    for prefix in [
        "var localSIPPortPort1 = toInterger(\"",
        "var localSIPPortPort2 = toInterger(\"",
    ] {
        if let Some(value) = slot(&mut source, prefix, "\")", "PORT")? {
            if data.peer.is_some() {
                return None;
            }
            data.peer = Some(u16::try_from(decimal(&value)?).ok().filter(|v| *v > 0)?);
        }
    }
    if source.contains("\\\" maxlength=2 size=7")
        && let Some(value) = slot(
            &mut source,
            "value=\\\"",
            "\\\" maxlength=2 size=7",
            "VALUE",
        )?
    {
        data.generated = Some(u8::try_from(decimal(&value)?).ok()?);
    }
    if let Some(value) = slot(&mut source, "var A=\"", "\" ;", "ROLE")? {
        if !matches!(value.as_str(), "admin" | "user" | "viewer") {
            return None;
        }
        data.role = Some(value);
    }
    if let Some(value) = slot(&mut source, "var B=", " ;", "FLAG")? {
        data.reset_disabled = Some(match value.as_str() {
            "0" => false,
            "1" => true,
            _ => return None,
        });
    }
    Some((source, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::native::NativeForm;
    use phone_core::console::Mode;

    fn fixture(peer: &str) -> (String, Url, Resources, Option<Plan>) {
        let port = format!(
            r#"var localSIPPortPort2 = toInterger("{peer}"); function checkSIPPort() {{ return Number(document.getElementsByName("P40")[0].value) !== localSIPPortPort2; }}"#
        );
        let generated =
            r#"document.write("<input name=\"P2399\" value=\"3\" maxlength=2 size=7>");"#;
        let initial = "function initial(){return;} initial();";
        let html = format!(
            r#"<script src="../scripts/jquery-min.js"></script><script src="../scripts/jquery.validate.min.js"></script><script src="../scripts/common.js"></script>
<form action="update" method="post" onsubmit="return checkSIPPort()"><input type="hidden" name="csrf_token" value="fixture-session"><input type="hidden" name="gnkey" value="fixture-key">
<label>Local SIP Port<input name="P40" value="5060" maxlength="5"></label>
<label>Authenticate Password<input type="password" name="P34" maxlength="64" readonly onfocus="javascript:this.removeAttribute('readonly');this.select();"></label>
<script>{generated}</script><input type="submit" name="update" value="Update"><input type="submit" name="apply" value="Apply"></form><script>{port}</script><script>{initial}</script>"#
        );
        let mut resources = Resources::default();
        let asset_hash = digest(b"fixture-resource");
        let assets: Vec<_> = RESOURCE_PATHS
            .iter()
            .map(|path| {
                resources.record(path, b"fixture-resource");
                (*path, asset_hash.as_str())
            })
            .collect();
        let fingerprints = [
            (
                Script::Fxs,
                digest(normalize(&port).unwrap_or_default().0.as_bytes()),
            ),
            (
                Script::OptionsFxs,
                digest(normalize(generated).unwrap().0.as_bytes()),
            ),
            (Script::Initial, digest(initial.as_bytes())),
        ];
        let scripts: Vec<_> = fingerprints
            .iter()
            .map(|(kind, hash)| (*kind, hash.as_str()))
            .collect();
        let url = Url::parse("http://device.example.invalid/cgi-bin/config_a1").unwrap();
        let plan = inspect(
            &Html::parse_document(&html),
            &url,
            &resources,
            &Manifest {
                assets: &assets,
                scripts: &scripts,
            },
        );
        (html, url, resources, plan)
    }

    #[test]
    fn reviewed_profiles_enable_password_editing_and_preserve_generated_controls() {
        let (html, url, _, plan) = fixture("5062");
        let form = NativeForm::parse_with_plan(&html, &url, true, Some(plan.unwrap())).unwrap();
        assert!(form.supports_mode(Mode::Save) && form.supports_mode(Mode::SaveAndApply));
        let password = form
            .fields()
            .iter()
            .find(|field| field.parameter == "P34")
            .unwrap();
        assert!(password.secret && !password.read_only && password.values.is_empty());
        let generated = form
            .fields()
            .iter()
            .find(|field| field.parameter == "P2399")
            .unwrap();
        let values = form
            .settings_values(
                &BTreeMap::from([
                    (password.id.clone(), vec!["changed-fixture-password".into()]),
                    (generated.id.clone(), vec!["5".into()]),
                ]),
                Mode::Save,
            )
            .unwrap();
        assert!(values.contains(&("P34".into(), "changed-fixture-password".into())));
        assert!(values.contains(&("P2399".into(), "5".into())));
        assert!(!values.iter().any(|(key, _)| key == "apply"));
    }

    #[test]
    fn changed_peer_ports_invalidate_review_and_collisions_are_refused() {
        let (html, url, _, plan) = fixture("5062");
        let form = NativeForm::parse_with_plan(&html, &url, true, Some(plan.unwrap())).unwrap();
        let port = form
            .fields()
            .iter()
            .find(|field| field.parameter == "P40")
            .unwrap();
        assert!(
            form.settings_values(
                &BTreeMap::from([(port.id.clone(), vec!["5062".into()])]),
                Mode::Save
            )
            .is_err()
        );
        assert!(
            form.settings_values(
                &BTreeMap::from([(port.id.clone(), vec!["5060x".into()])]),
                Mode::Save
            )
            .is_err()
        );
        let (other_html, _, _, other_plan) = fixture("5063");
        let other = NativeForm::parse_with_plan(&other_html, &url, true, Some(other_plan.unwrap()))
            .unwrap();
        assert!(!form.same_settings(&other));
        assert!(fixture("0").3.is_none());
        assert!(fixture("65536").3.is_none());
    }

    fn inspect_fixture(html: &str, url: &Url, resources: &Resources) -> Option<Plan> {
        let (original, _, _, _) = fixture("5062");
        let scripts: Vec<_> = Html::parse_document(&original)
            .select(&selector("script").unwrap())
            .filter(|node| node.value().attr("src").is_none())
            .map(|node| {
                let source = node.text().collect::<String>();
                let kind = if source.contains("checkSIPPort") {
                    Script::Fxs
                } else if source.contains("document.write") {
                    Script::OptionsFxs
                } else {
                    Script::Initial
                };
                (kind, digest(normalize(&source).unwrap().0.as_bytes()))
            })
            .collect();
        let scripts: Vec<_> = scripts
            .iter()
            .map(|(kind, hash)| (*kind, hash.as_str()))
            .collect();
        let hash = digest(b"fixture-resource");
        let assets: Vec<_> = RESOURCE_PATHS
            .iter()
            .map(|path| (*path, hash.as_str()))
            .collect();
        inspect(
            &Html::parse_document(html),
            url,
            resources,
            &Manifest {
                assets: &assets,
                scripts: &scripts,
            },
        )
    }

    #[test]
    fn altered_script_bindings_do_not_grant_write_authority() {
        let (html, url, resources, _) = fixture("5062");
        let original = inspect_fixture(&html, &url, &resources).unwrap();
        assert!(original.peer.is_some());
        for altered in [
            html.replace("name=\"P40\" value", "name=\"P41\" value"),
            html.replace(
                "type=\"password\" name=\"P34\"",
                "type=\"text\" name=\"P34\"",
            ),
            html.replace(
                "onsubmit=\"return checkSIPPort()\"",
                "onsubmit=\"return true\"",
            ),
            html.replace("<script>document.write", "</form><script>document.write"),
            html.replace(
                "<input type=\"submit\" name=\"update\"",
                "<input name=\"P2399\" value=\"4\"><input type=\"submit\" name=\"update\"",
            ),
        ] {
            assert!(inspect_fixture(&altered, &url, &resources).is_none());
        }
    }

    #[test]
    fn missing_or_changed_script_evidence_cannot_authorize_settings() {
        let (html, url, resources, _) = fixture("5062");
        assert!(inspect_fixture(&html, &url, &resources).is_some());
        assert!(inspect_fixture(&html, &url, &Resources::default()).is_none());
        let mut changed = resources;
        changed.record(RESOURCE_PATHS[0], b"changed-fixture-resource");
        assert!(inspect_fixture(&html, &url, &changed).is_none());
        let (_, _, resources, _) = fixture("5062");
        for altered in [
            html.replace(
                "../scripts/common.js",
                "https://other.example.invalid/scripts/common.js",
            ),
            html.replace("../scripts/common.js", "../scripts/common.js?changed=1"),
            html.replace("../scripts/common.js", "../scripts/unknown.js"),
            html.replace("<script>var local", "<script async>var local"),
            html.replace("return Number", "return false && Number"),
            format!("{html}<script>unknownBehavior();</script>"),
        ] {
            assert!(inspect_fixture(&altered, &url, &resources).is_none());
        }
    }

    #[test]
    fn reviewed_single_select_aliases_preserve_the_last_selected_value() {
        let (html, url, resources, _) = fixture("5062");
        let select = "<select name='P64'><option value='alias' selected>First alias</option><option value='other' selected>Other</option><option value='alias' selected>Last alias</option></select>";
        let html = html.replace(
            "<input type=\"submit\" name=\"update\"",
            &format!("{select}<input type=\"submit\" name=\"update\""),
        );
        let plan = inspect_fixture(&html, &url, &resources).unwrap();
        let form = NativeForm::parse_with_plan(&html, &url, true, Some(plan)).unwrap();
        let values = form.settings_values(&BTreeMap::new(), Mode::Save).unwrap();
        assert_eq!(
            values
                .iter()
                .filter(|(name, _)| name == "P64")
                .collect::<Vec<_>>(),
            vec![&("P64".into(), "alias".into())]
        );
        for altered in [
            html.replace("<select name='P64'>", "<select name='P64' multiple>"),
            html.replace(
                "value='alias' selected>Last",
                "value='alias' selected disabled>Last",
            ),
        ] {
            let plan = inspect_fixture(&altered, &url, &resources).unwrap();
            assert!(NativeForm::parse_with_plan(&altered, &url, true, Some(plan)).is_err());
        }
    }

    #[test]
    fn reviewed_tls_validation_preserves_the_native_sentinel_and_comparison() {
        let plan = Plan {
            page: Page::Advanced,
            peer: None,
            generated: vec![],
        };
        for minimum in ["99", "10", "11", "12"] {
            for maximum in ["99", "10", "11", "12"] {
                let values = vec![
                    ("P22293".into(), minimum.into()),
                    ("P22294".into(), maximum.into()),
                ];
                let expected = minimum == "99" || minimum <= maximum;
                assert_eq!(plan.validate(&values).is_ok(), expected);
            }
        }
        assert!(plan.validate(&[]).is_err());
        assert!(
            plan.validate(&[
                ("P22293".into(), "99".into()),
                ("P22293".into(), "10".into()),
                ("P22294".into(), "99".into())
            ])
            .is_err()
        );
    }

    #[test]
    fn unrelated_quoted_values_are_not_treated_as_generated_option_slots() {
        let source = r#"document.write("<input value=\"Reset\" type=button>");"#;
        let (normalized, data) = normalize(source).unwrap();
        assert_eq!(normalized, source);
        assert!(data.generated.is_none());
    }
}
