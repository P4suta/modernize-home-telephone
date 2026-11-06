const MAC_HEX_DIGITS: usize = 12;
const MIN_SECRET_BYTES: usize = 16;
const MAX_SECRET_BYTES: usize = 128;
const QUALIFIED_TEMPLATE_FIRMWARE: &str = "1.0.17.3";
const UNQUALIFIED_TEMPLATE_MESSAGE: &str = "the preserved parameter template declares firmware 1.0.17.3; qualify the observed firmware before provisioning";

use std::{collections::BTreeMap, net::Ipv4Addr};
#[derive(Clone, Copy)]
pub struct Input<'a> {
    pub firmware: &'a str,
    pub mac: &'a str,
    pub server: Ipv4Addr,
    pub password: &'a str,
}
pub fn parameters(input: &Input<'_>) -> anyhow::Result<BTreeMap<String, String>> {
    anyhow::ensure!(
        input.firmware == QUALIFIED_TEMPLATE_FIRMWARE,
        UNQUALIFIED_TEMPLATE_MESSAGE
    );
    anyhow::ensure!(
        input.mac.len() == MAC_HEX_DIGITS && input.mac.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "MAC must contain exactly twelve hexadecimal digits"
    );
    anyhow::ensure!(
        !input.server.is_unspecified()
            && !input.server.is_loopback()
            && !input.server.is_multicast()
            && !input.server.is_broadcast(),
        "SIP destination must be a reachable unicast IPv4 address"
    );
    anyhow::ensure!(
        input.password.len() >= MIN_SECRET_BYTES
            && input.password.len() <= MAX_SECRET_BYTES
            && input
                .password
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)),
        "gateway secret must contain 16 to 128 ASCII letters, digits, hyphens, or underscores"
    );
    Ok([
        ("P271", "1".into()),
        ("P47", input.server.to_string()),
        ("P35", "handset".into()),
        ("P36", "handset".into()),
        ("P34", input.password.into()),
        ("P31", "1".into()),
        ("P57", "0".into()),
        ("P2346", "1".into()),
        ("P401", "1".into()),
        ("P747", input.server.to_string()),
        ("P735", "fxo".into()),
        ("P736", "fxo".into()),
        ("P734", input.password.into()),
        ("P731", "1".into()),
        ("P757", "0".into()),
        ("P2446", "1".into()),
        ("P842", "0".into()),
        ("P3304", "1".into()),
        ("P3305", "home".into()),
        ("P3306", input.server.to_string()),
        ("P3307", "5060".into()),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect())
}
pub fn render(input: &Input<'_>) -> anyhow::Result<String> {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gs_provision version=\"1\">\n<mac>{}</mac>\n<config version=\"1\">\n",
        input.mac.to_ascii_lowercase()
    );
    for (key, value) in parameters(input)? {
        xml.push_str(&format!("<{key}>{value}</{key}>\n"));
    }
    xml.push_str("</config>\n</gs_provision>\n");
    Ok(xml)
}
