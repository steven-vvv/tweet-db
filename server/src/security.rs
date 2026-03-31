use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy)]
pub enum TokenKind {
    Session,
}

impl TokenKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IssuedCompoundSecret {
    pub selector: Uuid,
    pub verifier: String,
    pub verifier_mac: Vec<u8>,
}

impl IssuedCompoundSecret {
    pub fn compound_value(&self) -> String {
        format!("{}.{}", self.selector, self.verifier)
    }
}

pub fn issue_compound_secret(key: &[u8; 32], kind: TokenKind) -> IssuedCompoundSecret {
    let selector = Uuid::now_v7();
    let verifier = random_urlsafe_string(32);
    let verifier_mac = token_verifier_mac(key, kind, selector, &verifier);

    IssuedCompoundSecret {
        selector,
        verifier,
        verifier_mac,
    }
}

pub fn parse_compound_secret(value: &str) -> Option<(Uuid, String)> {
    let mut parts = value.splitn(2, '.');
    let selector = parts.next()?;
    let verifier = parts.next()?.to_owned();
    Some((Uuid::parse_str(selector).ok()?, verifier))
}

pub fn token_verifier_mac(
    key: &[u8; 32],
    kind: TokenKind,
    selector: Uuid,
    verifier: &str,
) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts arbitrary key lengths here");
    mac.update(format!("{}:{selector}:{verifier}", kind.as_str()).as_bytes());
    mac.finalize().into_bytes().to_vec()
}

pub fn sha256_digest(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

pub fn pkce_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn random_urlsafe_string(byte_len: usize) -> String {
    let bytes = (0..byte_len).map(|_| rand_byte()).collect::<Vec<_>>();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn rand_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch");
    let nanos = now.subsec_nanos();
    (nanos % 255) as u8
}
