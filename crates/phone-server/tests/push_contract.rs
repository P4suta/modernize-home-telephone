use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use openssl::{
    bn::BigNum,
    ec::{EcGroup, EcKey},
    ecdsa::EcdsaSig,
    nid::Nid,
};
use phone_server::{
    model::{PushKeys, PushSubscription},
    push,
};

#[test]
fn push_messages_are_recipient_encrypted_and_signed_for_the_exact_push_origin() {
    let key = EcKey::generate(&EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap()).unwrap();
    let pem = String::from_utf8(key.private_key_to_pem().unwrap()).unwrap();
    let (recipient, auth) = ece::generate_keypair_and_auth_secret().unwrap();
    let subscription = PushSubscription {
        endpoint: "https://push.example.invalid/send/private-token".into(),
        keys: PushKeys {
            p256dh: URL_SAFE_NO_PAD.encode(recipient.pub_as_raw().unwrap()),
            auth: URL_SAFE_NO_PAD.encode(auth),
        },
    };
    let encoded = push::encode(
        &pem,
        "mailto:owner@example.invalid",
        &subscription,
        b"private payload",
        1000,
    )
    .unwrap();
    assert!(
        !encoded
            .body
            .windows(15)
            .any(|bytes| bytes == b"private payload")
    );
    assert_eq!(
        ece::decrypt(&recipient.raw_components().unwrap(), &auth, &encoded.body).unwrap(),
        b"private payload"
    );
    let authorization = encoded.authorization.strip_prefix("vapid t=").unwrap();
    let (token, public) = authorization.split_once(", k=").unwrap();
    assert_eq!(public, push::public_key(&pem).unwrap());
    let parts: Vec<_> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(claims["aud"], "https://push.example.invalid");
    assert_eq!(claims["exp"], 1000 + 43200);
    assert_eq!(claims["sub"], "mailto:owner@example.invalid");
    let raw = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    assert_eq!(raw.len(), 64);
    let signature = EcdsaSig::from_private_components(
        BigNum::from_slice(&raw[..32]).unwrap(),
        BigNum::from_slice(&raw[32..]).unwrap(),
    )
    .unwrap();
    assert!(
        signature
            .verify(
                &openssl::sha::sha256(format!("{}.{}", parts[0], parts[1]).as_bytes()),
                &key
            )
            .unwrap()
    );
    let mut corrupted = encoded.body.clone();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 1;
    assert!(ece::decrypt(&recipient.raw_components().unwrap(), &auth, &corrupted).is_err());
}
