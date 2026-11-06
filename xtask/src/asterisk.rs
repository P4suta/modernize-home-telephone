use std::{collections::BTreeMap, net::IpAddr};

const MIN_SECRET_BYTES: usize = 16;
const MAX_SECRET_BYTES: usize = 128;
const ARI_HTTP_CONFIGURATION: &str = "[general]\nenabled=yes\nbindaddr=127.0.0.1\nbindport=8088\n";
const PUBLIC_UDP_BIND: &str = "bind=0.0.0.0:5060";
const LAB_UDP_BIND: &str = "bind=127.0.0.1:5060";
const PJSIP_TRANSPORT_CONFIGURATION: &str = "[global]\ntype=global\nuser_agent=HomeTelephone\nendpoint_identifier_order=auth_username,username,ip\n[udp]\ntype=transport\nprotocol=udp\nbind=0.0.0.0:5060\nexternal_media_address={}\nexternal_signaling_address={}\n[ws]\ntype=transport\nprotocol=ws\nbind=127.0.0.1\nexternal_media_address={}\n";
const LAB_CARRIER_CONFIGURATION: &str = "[lab-carrier]\ntype=aor\ncontact=sip:127.0.0.1:5070\n[lab-carrier]\ntype=endpoint\ncontext=home-carrier\ndisallow=all\nallow=ulaw\naors=lab-carrier\ndirect_media=no\nrtp_symmetric=yes\nforce_rport=yes\n[lab-carrier-identify]\ntype=identify\nendpoint=lab-carrier\nmatch=127.0.0.1\n";

pub struct Configuration<'a> {
    pub address: IpAddr,
    pub ari_password: &'a str,
    pub browser_password: &'a str,
    pub gateway_password: &'a str,
    pub lab: bool,
}

pub fn render(config: &Configuration<'_>) -> anyhow::Result<BTreeMap<String, String>> {
    for secret in [
        config.ari_password,
        config.browser_password,
        config.gateway_password,
    ] {
        anyhow::ensure!(
            secret.len() >= MIN_SECRET_BYTES
                && secret.len() <= MAX_SECRET_BYTES
                && secret
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
            "Asterisk credentials must contain 16 to 128 ASCII letters or digits"
        );
    }
    let mut files = BTreeMap::new();
    files.insert("asterisk.conf".into(), "[directories]\nastetcdir=/etc/asterisk\nastmoddir=/usr/lib/asterisk/modules\nastvarlibdir=/var/lib/asterisk\nastdbdir=/var/cache/asterisk\nastkeydir=/etc/asterisk/keys\nastdatadir=/var/lib/asterisk\nastagidir=/var/lib/asterisk/agi-bin\nastspooldir=/var/spool/asterisk\nastrundir=/var/run/asterisk\nastlogdir=/var/log/asterisk\n[options]\nverbose=0\ndebug=0\n[compat]\npbx_realtime=1.6\nres_agi=1.6\napp_set=1.6\n".into());
    files.insert("modules.conf".into(), "[modules]\nautoload=yes\nnoload=chan_skinny.so\nnoload=chan_mgcp.so\nnoload=res_hep.so\nnoload=res_hep_pjsip.so\nnoload=res_hep_rtcp.so\nnoload=res_snmp.so\nnoload=res_phoneprov.so\n".into());
    files.insert(
        "logger.conf".into(),
        "[general]\n[logfiles]\nconsole=notice,warning,error\n".into(),
    );
    files.insert("cdr.conf".into(), "[general]\nenable=no\n".into());
    files.insert("cel.conf".into(), "[general]\nenable=no\n".into());
    files.insert("http.conf".into(), ARI_HTTP_CONFIGURATION.into());
    files.insert("ari.conf".into(), format!("[general]\nenabled=yes\npretty=no\n[homephone]\ntype=user\nread_only=no\npassword={}\n", config.ari_password));
    files.insert(
        "rtp.conf".into(),
        "[general]\nrtpstart=10000\nrtpend=10099\nicesupport=yes\nstrictrtp=yes\n".into(),
    );
    files.insert(
        "musiconhold.conf".into(),
        "[default]\nmode=files\ndirectory=/var/lib/asterisk/moh\nsort=alpha\n".into(),
    );
    let mut pjsip = PJSIP_TRANSPORT_CONFIGURATION.replace("{}", &config.address.to_string());
    for (name, password, context, webrtc) in [
        ("fxo", config.gateway_password, "home-carrier", false),
        ("handset", config.gateway_password, "home-outbound", false),
        ("browser", config.browser_password, "home-outbound", true),
    ] {
        pjsip.push_str(&format!("[{name}-auth]\ntype=auth\nauth_type=userpass\nusername={name}\npassword={password}\n[{name}]\ntype=aor\nmax_contacts=1\nremove_existing=yes\nqualify_frequency=30\n[{name}]\ntype=endpoint\ncontext={context}\ndisallow=all\nallow=ulaw,alaw\nauth={name}-auth\noutbound_auth={name}-auth\naors={name}\ndirect_media=no\nrtp_symmetric=yes\nforce_rport=yes\nrewrite_contact=yes\ndtmf_mode=rfc4733\nlanguage=ja\n"));
        if webrtc {
            pjsip.push_str("webrtc=yes\nmedia_encryption=dtls\ndtls_verify=fingerprint\ndtls_setup=actpass\ndtls_cert_file=/etc/asterisk/keys/dtls.crt\ndtls_private_key=/etc/asterisk/keys/dtls.key\n");
        }
    }
    if config.lab {
        pjsip = pjsip.replace(PUBLIC_UDP_BIND, LAB_UDP_BIND);
        pjsip.push_str(LAB_CARRIER_CONFIGURATION);
    }
    files.insert("pjsip.conf".into(), pjsip);
    let mut dialplan = "[general]\nstatic=yes\nwriteprotect=yes\n[home-carrier]\nexten => _.,1,NoOp(Incoming home call)\n same => n,Stasis(homephone,inbound)\n same => n,Dial(PJSIP/handset,30)\n same => n,Hangup()\n[home-outbound]\nexten => _[+0-9].,1,Stasis(homephone,outbound,${EXTEN})\n same => n,Hangup()\nexten => _[0-9],1,Stasis(homephone,outbound,${EXTEN})\n same => n,Hangup()\n[home-endpoint]\nexten => browser,1,Dial(PJSIP/browser,30)\n same => n,Hangup()\n".to_owned();
    if config.lab {
        dialplan.push_str("[home-lab]\nexten => _.,1,Answer()\n same => n,Wait(1)\n same => n,Playback(homephone/caller)\n same => n,Goto(2)\n");
    }
    files.insert("extensions.conf".into(), dialplan);
    Ok(files)
}
