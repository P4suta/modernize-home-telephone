use super::{
    model::{Help, HelpReference, HelpTopic, ParameterReference},
    native::NativeForm,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn catalogue() -> Vec<Help> {
    let references: Vec<ParameterReference> =
        serde_json::from_str(include_str!("../../../../console/src/lib/parameters.json"))
            .expect("the checked parameter references are valid");
    let topics: Vec<HelpTopic> =
        serde_json::from_str(include_str!("../../../../console/src/lib/settings.json"))
            .expect("the checked help topics are valid");
    let firmware: Vec<Help> = serde_json::from_str(include_str!(
        "../../../../console/src/lib/firmware-parameters.json"
    ))
    .expect("the checked firmware references are valid");
    append_firmware_help(
        build_catalogue(&references, &topics).expect("every parameter has complete help"),
        &firmware,
    )
    .expect("firmware help must remain complete and distinct")
}

pub fn build_catalogue(
    references: &[ParameterReference],
    topics: &[HelpTopic],
) -> anyhow::Result<Vec<Help>> {
    let topics_by_key: BTreeMap<_, _> = topics
        .iter()
        .map(|topic| (topic.key.as_str(), topic))
        .collect();
    anyhow::ensure!(
        !references.is_empty() && topics_by_key.len() == topics.len(),
        "empty references or duplicate topics"
    );
    let mut ids = BTreeSet::new();
    references.iter().map(|parameter| {
        anyhow::ensure!(ids.insert(&parameter.id) && !parameter.title.is_empty() && parameter.source_line > 0
            && parameter.ranges.iter().all(|range| range.min <= range.max), "invalid parameter facts");
        let topic = topics_by_key.get(parameter.topic.as_str()).ok_or_else(|| anyhow::anyhow!("missing parameter explanation"))?;
        let section = match parameter.section.as_str() {
            "basic" => "基本設定", "advanced" => "詳細設定", "fxs" => "電話機（FXS）", "fxo" => "電話回線（FXO）",
            _ => anyhow::bail!("unknown parameter section"),
        };
        let aliases = topic.aliases.clone();
        let condition = parameter.ranges.iter().map(|range| {
            if range.min == range.max { range.min.to_string() } else { format!("{}〜{}", range.min, range.max) }
        }).collect::<Vec<_>>().join("、");
        Ok(Help {
            key: format!("parameter-{}", parameter.id.to_ascii_lowercase()),
            title: parameter.title.clone(),
            aliases,
            parameters: vec![if parameter.id == "8467" { "P8467".into() } else { parameter.id.clone() }],
            category: format!("{section} / {}", topic.title),
            summary: topic.summary.clone(),
            input: if condition.is_empty() { topic.input.clone() } else { format!("資料の数値範囲: {condition}。\n{}", topic.input) },
            default_value: parameter.default_value.clone().unwrap_or_else(|| "資料で工場出荷時の値を確定できません。\nテンプレートの記載値と接続中の値を区別して確認してください。".into()),
            recommendation: topic.recommendation.clone(),
            impact: topic.impact.clone(),
            source: topic.source.clone(),
            template_value: parameter.template_value.clone(),
            reference: HelpReference::Template { firmware: "1.0.17.3".into(), line: parameter.source_line },
            ranges: parameter.ranges.clone(),
            note: parameter.note.clone(),
            read_only: parameter.read_only,
        })
    }).collect()
}

pub fn append_firmware_help(
    mut catalogue: Vec<Help>,
    firmware: &[Help],
) -> anyhow::Result<Vec<Help>> {
    anyhow::ensure!(!firmware.is_empty(), "firmware help cannot be empty");
    let mut keys: BTreeSet<String> = catalogue.iter().map(|help| help.key.clone()).collect();
    let mut parameters: BTreeSet<String> = catalogue
        .iter()
        .flat_map(|help| help.parameters.clone())
        .collect();
    for help in firmware {
        anyhow::ensure!(
            matches!(&help.reference, HelpReference::Device { firmware } if firmware == "1.0.19.6")
                && help.template_value.is_none()
                && help.parameters.len() == 1
                && keys.insert(help.key.clone())
                && parameters.insert(help.parameters[0].clone())
                && [
                    &help.title,
                    &help.summary,
                    &help.input,
                    &help.default_value,
                    &help.recommendation,
                    &help.impact
                ]
                .iter()
                .all(|value| !value.trim().is_empty())
                && help.ranges.iter().all(|range| range.min <= range.max),
            "invalid or duplicate firmware help"
        );
        catalogue.push(help.clone());
    }
    Ok(catalogue)
}

pub fn valid_values(help: &Help, values: &[String]) -> bool {
    !help.read_only
        && (help.ranges.is_empty()
            || values.iter().all(|value| {
                let digits = value.strip_prefix('-').unwrap_or(value);
                !digits.is_empty()
                    && digits.bytes().all(|byte| byte.is_ascii_digit())
                    && value.parse::<i32>().is_ok_and(|number| {
                        help.ranges
                            .iter()
                            .any(|range| range.min <= number && number <= range.max)
                    })
            }))
}

pub fn find<'a>(
    catalogue: &'a [Help],
    form: &NativeForm,
    id: &str,
    label: &str,
) -> Option<&'a Help> {
    let parameter = form.parameter(id)?;
    catalogue
        .iter()
        .find(|help| help.parameters.iter().any(|value| value == parameter))
        .or_else(|| {
            if parameter
                .strip_prefix('P')
                .is_some_and(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return None;
            }
            catalogue
                .iter()
                .find(|help| help.aliases.iter().any(|value| value == label))
        })
}
