//! Self-hosted [Altcha](https://altcha.org) proof-of-work captcha.
//!
//! The widget (vendored at `assets/static/altcha.min.js`, CSP-friendly
//! `dist_external` build) fetches a challenge from `/captcha/challenge`,
//! brute-forces the secret number in a Web Worker, and submits the solution
//! as a base64 JSON payload in a hidden `altcha` form field. No third-party
//! service is involved.
//!
//! Server side: challenges are `SHA-256(salt + number)` puzzles signed with
//! HMAC-SHA256 under a boot-ephemeral key. Verification checks the
//! solution, the signature, the expiry embedded in the salt, and replays
//! (each solved challenge is single-use). Challenges do not survive a
//! restart — the widget simply fetches a fresh one.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ALGORITHM: &str = "SHA-256";
/// Upper bound for the secret number; tunes client solve time (well under a
/// second on commodity hardware at 100k).
pub const MAX_NUMBER: u64 = 100_000;
const CHALLENGE_TTL_SECS: u64 = 600;

/// A challenge in the wire format the Altcha widget expects.
#[derive(Debug, Clone, Serialize)]
pub struct Challenge {
    pub algorithm: String,
    pub challenge: String,
    pub maxnumber: u64,
    pub salt: String,
    pub signature: String,
}

/// The solution payload the widget submits (base64-encoded JSON).
#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    algorithm: String,
    challenge: String,
    number: u64,
    salt: String,
    signature: String,
}

/// Why a captcha payload was rejected.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    #[error("malformed captcha payload")]
    Malformed,
    #[error("unsupported captcha algorithm")]
    Algorithm,
    #[error("captcha challenge expired")]
    Expired,
    #[error("captcha solution incorrect")]
    WrongSolution,
    #[error("captcha signature invalid")]
    BadSignature,
    #[error("captcha already used")]
    Replayed,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// HMAC-SHA256 per RFC 2104 (verified against RFC 4231 vectors below).
/// Implemented over the existing `sha2` dependency instead of pulling in a
/// crate whose `digest` version may diverge from ours.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize()
        .into()
}

/// Constant-time string equality (hex digests) — avoids leaking how many
/// leading characters of a signature matched.
fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Boot-ephemeral signing key from the OS CSPRNG (via `UUIDv4`).
fn signing_key() -> &'static [u8; 32] {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        key[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        key
    })
}

fn random_number(max: u64) -> u64 {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    let mut eight = [0u8; 8];
    eight.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(eight) % (max + 1)
}

fn challenge_hash(salt: &str, number: u64) -> String {
    hex(&Sha256::digest(format!("{salt}{number}")))
}

/// Creates a fresh signed challenge for the widget.
#[must_use]
pub fn create_challenge() -> Challenge {
    let expires = now_secs() + CHALLENGE_TTL_SECS;
    let salt = format!("{}?expires={expires}", hex(uuid::Uuid::new_v4().as_bytes()));
    let number = random_number(MAX_NUMBER);
    let challenge = challenge_hash(&salt, number);
    let signature = hex(&hmac_sha256(signing_key(), challenge.as_bytes()));
    Challenge {
        algorithm: ALGORITHM.to_string(),
        challenge,
        maxnumber: MAX_NUMBER,
        salt,
        signature,
    }
}

fn parse_expires(salt: &str) -> Option<u64> {
    let (_, params) = salt.split_once('?')?;
    params
        .split('&')
        .find_map(|kv| kv.strip_prefix("expires="))
        .and_then(|v| v.parse().ok())
}

/// Marks a challenge as used. Returns false when it was already consumed.
fn mark_used(challenge: &str, expires: u64) -> bool {
    static USED: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let store = USED.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut used) = store.lock() else {
        // A poisoned lock fails closed: treat everything as replayed.
        return false;
    };
    let now = now_secs();
    used.retain(|_, exp| *exp > now);
    used.insert(challenge.to_string(), expires).is_none()
}

/// Verifies a widget payload (the base64 JSON from the `altcha` form field).
/// A challenge verifies successfully exactly once.
///
/// # Errors
///
/// Returns the specific [`VerifyError`] — callers usually map any error to a
/// generic "captcha failed" response.
pub fn verify_payload(payload_b64: &str) -> Result<(), VerifyError> {
    let json = base64::engine::general_purpose::STANDARD
        .decode(payload_b64.trim())
        .map_err(|_| VerifyError::Malformed)?;
    let payload: Payload = serde_json::from_slice(&json).map_err(|_| VerifyError::Malformed)?;

    if payload.algorithm != ALGORITHM {
        return Err(VerifyError::Algorithm);
    }
    if payload.number > MAX_NUMBER {
        return Err(VerifyError::WrongSolution);
    }
    let expires = parse_expires(&payload.salt).ok_or(VerifyError::Malformed)?;
    if expires <= now_secs() {
        return Err(VerifyError::Expired);
    }
    // The signature proves WE issued this salt/challenge; check it before
    // trusting anything else about the payload.
    let expected_sig = hex(&hmac_sha256(signing_key(), payload.challenge.as_bytes()));
    if !ct_eq(&expected_sig, &payload.signature) {
        return Err(VerifyError::BadSignature);
    }
    if challenge_hash(&payload.salt, payload.number) != payload.challenge {
        return Err(VerifyError::WrongSolution);
    }
    if !mark_used(&payload.challenge, expires) {
        return Err(VerifyError::Replayed);
    }
    Ok(())
}

/// Solves a challenge by brute force, returning the widget-style payload.
///
/// This is the client's job — exposed for tests and tooling only; it
/// grants nothing an attacker couldn't compute themselves.
#[must_use]
pub fn solve_challenge(challenge: &Challenge) -> Option<String> {
    (0..=challenge.maxnumber)
        .find(|n| challenge_hash(&challenge.salt, *n) == challenge.challenge)
        .map(|number| {
            let payload = Payload {
                algorithm: challenge.algorithm.clone(),
                challenge: challenge.challenge.clone(),
                number,
                salt: challenge.salt.clone(),
                signature: challenge.signature.clone(),
            };
            base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec(&payload).unwrap_or_default())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 1.
    #[test]
    fn hmac_sha256_rfc4231_case1() {
        let key = [0x0b_u8; 20];
        let out = hmac_sha256(&key, b"Hi There");
        assert_eq!(
            hex(&out),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    /// RFC 4231 test case 2 ("Jefe").
    #[test]
    fn hmac_sha256_rfc4231_case2() {
        let out = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&out),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    /// RFC 4231 test case 6 (key longer than the block size).
    #[test]
    fn hmac_sha256_rfc4231_case6() {
        let key = [0xaa_u8; 131];
        let out = hmac_sha256(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex(&out),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn solved_challenge_verifies_once_then_replays() {
        let challenge = create_challenge();
        let payload = solve_challenge(&challenge).expect("solvable");
        assert_eq!(verify_payload(&payload), Ok(()));
        assert_eq!(verify_payload(&payload), Err(VerifyError::Replayed));
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let challenge = create_challenge();
        let mut tampered = challenge.clone();
        tampered.signature = hex(&[0u8; 32]);
        let payload = solve_challenge(&tampered).expect("solvable");
        assert_eq!(verify_payload(&payload), Err(VerifyError::BadSignature));
    }

    #[test]
    fn self_made_challenge_without_our_key_is_rejected() {
        // An attacker fabricating their own salt/challenge/solution still
        // fails the HMAC check.
        let salt = "deadbeef?expires=9999999999".to_string();
        let number = 7;
        let fabricated = Challenge {
            algorithm: ALGORITHM.to_string(),
            challenge: challenge_hash(&salt, number),
            maxnumber: MAX_NUMBER,
            salt,
            signature: hex(&[7u8; 32]),
        };
        let payload = solve_challenge(&fabricated).expect("solvable");
        assert_eq!(verify_payload(&payload), Err(VerifyError::BadSignature));
    }

    #[test]
    fn expired_challenge_is_rejected() {
        let salt = format!("{}?expires={}", hex(&[1u8; 16]), now_secs() - 5);
        let number = 3;
        let challenge_str = challenge_hash(&salt, number);
        let signature = hex(&hmac_sha256(signing_key(), challenge_str.as_bytes()));
        let fabricated = Challenge {
            algorithm: ALGORITHM.to_string(),
            challenge: challenge_str,
            maxnumber: MAX_NUMBER,
            salt,
            signature,
        };
        let payload = solve_challenge(&fabricated).expect("solvable");
        assert_eq!(verify_payload(&payload), Err(VerifyError::Expired));
    }

    #[test]
    fn garbage_payloads_are_malformed() {
        assert_eq!(verify_payload("not base64!!"), Err(VerifyError::Malformed));
        assert_eq!(
            verify_payload(&base64::engine::general_purpose::STANDARD.encode("{}")),
            Err(VerifyError::Malformed)
        );
    }
}
