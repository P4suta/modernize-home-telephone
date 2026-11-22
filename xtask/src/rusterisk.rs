pub use crate::asterisk::Configuration;
use crate::operations::{finish_cleanup, private_directory, private_file, sync_parent};
use anyhow::{Context, ensure};
use std::{collections::BTreeMap, fs, net::IpAddr, path::Path};

pub fn render(config: &Configuration<'_>) -> anyhow::Result<BTreeMap<String, String>> {
    crate::asterisk::validate_credentials(config)?;
    let bind = if config.lab { "127.0.0.1" } else { "0.0.0.0" };
    let mut sip = format!(
        "[udp]\ntype=transport\nprotocol=udp\nbind={bind}:5060\nexternal_media_address={address}\n[ws]\ntype=transport\nprotocol=ws\nbind=127.0.0.1\nexternal_media_address={address}\n",
        address = config.address
    );
    for (name, transport, context, password) in [
        ("fxo", "udp", "home-carrier", config.gateway_password),
        ("handset", "udp", "home-outbound", config.gateway_password),
        ("browser", "ws", "home-outbound", config.browser_password),
    ] {
        sip.push_str(&format!("[{name}-auth]\ntype=auth\nusername={name}\npassword={password}\n[{name}]\ntype=aor\nmax_contacts=1\n[{name}]\ntype=endpoint\ntransport={transport}\nauth={name}-auth\noutbound_auth={name}-auth\naors={name}\ncontext={context}\ndisallow=all\nallow=ulaw,alaw\ndtmf_mode=rfc4733\n"));
        if name == "browser" {
            sip.push_str("webrtc=yes\n");
        }
    }
    if config.lab {
        for (name, port, context) in [
            ("lab-carrier", 5070, "home-carrier"),
            ("lab-caller", 5060, "home-lab"),
        ] {
            sip.push_str(&format!("[{name}-auth]\ntype=auth\nusername={name}\npassword={password}\n[{name}]\ntype=aor\ncontact=sip:127.0.0.1:{port}\n[{name}]\ntype=endpoint\ntransport=udp\nauth={name}-auth\noutbound_auth={name}-auth\naors={name}\ncontext={context}\ndisallow=all\nallow=ulaw\n", password = config.gateway_password));
        }
    }
    Ok(BTreeMap::from([
        (
            "http.conf".into(),
            "[general]\nenabled=yes\nbindaddr=127.0.0.1\nbindport=8088\n".into(),
        ),
        (
            "ari.conf".into(),
            format!(
                "[general]\nenabled=yes\n[homephone]\ntype=user\nread_only=no\npassword={}\n",
                config.ari_password
            ),
        ),
        (
            "rtp.conf".into(),
            "[general]\nrtpstart=10000\nrtpend=10099\n".into(),
        ),
        (
            "musiconhold.conf".into(),
            "[default]\nmode=files\ndirectory=moh\nsort=alpha\n".into(),
        ),
        (
            "extensions.conf".into(),
            crate::asterisk::dialplan(config.lab),
        ),
        ("pjsip.conf".into(), sip),
    ]))
}

pub fn write(directory: &Path, files: &BTreeMap<String, String>) -> anyhow::Result<()> {
    let target = directory.join("rusterisk");
    let staging = directory.join(format!(".rusterisk-{}.partial", uuid::Uuid::new_v4()));
    private_directory(&staging)?;
    let result = (|| {
        for (name, body) in files {
            private_file(&staging.join(name), body.as_bytes())?;
        }
        fs::rename(&staging, &target)?;
        sync_parent(&target)
    })();
    if result.is_err() && staging.exists() {
        return finish_cleanup(result, fs::remove_dir_all(staging));
    }
    result
}

pub fn prepare_existing(directory: &Path) -> anyhow::Result<()> {
    let directory = directory.join("config");
    let native = directory.join("rusterisk");
    match fs::symlink_metadata(&native) {
        Ok(metadata) => {
            ensure!(metadata.is_dir(), "config/rusterisk must be a directory");
            for name in [
                "http.conf",
                "ari.conf",
                "pjsip.conf",
                "rtp.conf",
                "extensions.conf",
                "musiconhold.conf",
            ] {
                ensure!(
                    fs::symlink_metadata(native.join(name))
                        .with_context(|| format!("missing config/rusterisk/{name}"))?
                        .is_file(),
                    "config/rusterisk/{name} must be a regular file"
                );
            }
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let boot: toml::Value = toml::from_str(&fs::read_to_string(directory.join("homephone.toml"))?)?;
    let lab = match boot.get("mode").and_then(toml::Value::as_str) {
        Some("lab") => true,
        Some("production") => false,
        _ => anyhow::bail!("unknown telephone installation mode"),
    };
    let secrets = fs::read_to_string(directory.join("secrets.env"))?;
    let credential = |key: &str| -> anyhow::Result<&str> {
        let mut values = secrets
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter_map(|(name, value)| (name == key).then_some(value));
        let value = values.next().context("missing telephone credential")?;
        ensure!(values.next().is_none(), "duplicate telephone credential");
        Ok(value)
    };
    let sip = fs::read_to_string(directory.join("pjsip.conf"))?;
    let address: IpAddr = sip
        .lines()
        .find_map(|line| line.strip_prefix("external_media_address="))
        .context("missing advertised gateway address")?
        .parse()?;
    let input = Configuration {
        address,
        lab,
        ari_password: credential("HOMEPHONE_ARI_PASSWORD")?,
        browser_password: credential("HOMEPHONE_SIP_PASSWORD")?,
        gateway_password: credential("HOMEPHONE_GATEWAY_PASSWORD")?,
    };
    let mut files = render(&input)?;
    let previous = crate::asterisk::render(&input)?;
    for (name, expected) in previous {
        if name != "extensions.conf" {
            let installed = fs::read_to_string(directory.join(&name))?;
            ensure!(
                installed == expected
                    || (name == "pjsip.conf"
                        && installed
                            == expected
                                .replace("identify_by=auth_username,username\n", "")
                                .replace("match=127.0.0.1:5070\n", "match=127.0.0.1\n")),
                "custom {name} requires an explicit config/rusterisk configuration"
            );
        }
    }
    files.insert(
        "extensions.conf".into(),
        fs::read_to_string(directory.join("extensions.conf"))?,
    );
    write(&directory, &files)
}
