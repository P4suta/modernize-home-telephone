const MAX_PAYLOAD_BYTES: usize = 2048;
const PUBLIC_KEY_BYTES: usize = 65;
const AUTH_SECRET_BYTES: usize = 16;
const VAPID_LIFETIME_SECONDS: u64 = 43200;
const ECDSA_SCALAR_BYTES: usize = 32;
const ECDSA_SIGNATURE_BYTES: usize = 64;

use crate::model::PushSubscription;
use anyhow::{Context, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use openssl::{
    bn::BigNumContext,
    ec::PointConversionForm,
    ecdsa::EcdsaSig,
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
    sign::Signer,
};
use serde_json::json;

pub struct Encoded {
    pub authorization: String,
    pub body: Vec<u8>,
}
fn key(pem: &str) -> anyhow::Result<PKey<Private>> {
    let key = PKey::private_key_from_pem(pem.as_bytes())?;
    ensure!(
        key.ec_key()?.group().curve_name() == Some(Nid::X9_62_PRIME256V1),
        "VAPID requires a P-256 key"
    );
    Ok(key)
}
pub fn public_key(pem: &str) -> anyhow::Result<String> {
    let key = key(pem)?.ec_key()?;
    let mut context = BigNumContext::new()?;
    let bytes =
        key.public_key()
            .to_bytes(key.group(), PointConversionForm::UNCOMPRESSED, &mut context)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
pub fn encode(
    pem: &str,
    subject: &str,
    subscription: &PushSubscription,
    payload: &[u8],
    now: u64,
) -> anyhow::Result<Encoded> {
    let endpoint = url::Url::parse(&subscription.endpoint)?;
    ensure!(
        endpoint.scheme() == "https"
            && endpoint.host_str().is_some()
            && endpoint.username().is_empty()
            && endpoint.password().is_none(),
        "invalid push endpoint"
    );
    let subject_url = url::Url::parse(subject)?;
    ensure!(
        matches!(subject_url.scheme(), "mailto" | "https"),
        "invalid VAPID contact URI"
    );
    ensure!(
        payload.len() <= MAX_PAYLOAD_BYTES,
        "push payload exceeded its bound"
    );
    let recipient = URL_SAFE_NO_PAD.decode(&subscription.keys.p256dh)?;
    let auth = URL_SAFE_NO_PAD.decode(&subscription.keys.auth)?;
    ensure!(
        recipient.len() == PUBLIC_KEY_BYTES && auth.len() == AUTH_SECRET_BYTES,
        "invalid push encryption key"
    );
    let body = ece::encrypt(&recipient, &auth, payload)?;
    let header = URL_SAFE_NO_PAD.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims=URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({"aud":endpoint.origin().ascii_serialization(),"exp":now.checked_add(VAPID_LIFETIME_SECONDS).context("VAPID expiry overflow")?,"sub":subject}))?);
    let input = format!("{header}.{claims}");
    let key = key(pem)?;
    let mut signer = Signer::new(MessageDigest::sha256(), &key)?;
    signer.update(input.as_bytes())?;
    let signature = EcdsaSig::from_der(&signer.sign_to_vec()?)?;
    let r = signature.r().to_vec();
    let s = signature.s().to_vec();
    ensure!(
        r.len() <= ECDSA_SCALAR_BYTES && s.len() <= ECDSA_SCALAR_BYTES,
        "invalid ECDSA signature width"
    );
    let mut signature = [0_u8; 64];
    signature[ECDSA_SCALAR_BYTES - r.len()..ECDSA_SCALAR_BYTES].copy_from_slice(&r);
    signature[ECDSA_SIGNATURE_BYTES - s.len()..].copy_from_slice(&s);
    Ok(Encoded {
        authorization: format!(
            "vapid t={input}.{}, k={}",
            URL_SAFE_NO_PAD.encode(signature),
            public_key(pem)?
        ),
        body,
    })
}
