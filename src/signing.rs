use hmac::{Hmac, Mac};
use serde::{Serialize, de::DeserializeOwned};
use sha2::Sha256;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("signing secret is not configured")]
    MissingSigningSecret,
    #[error("signed payload is malformed")]
    MalformedToken,
    #[error("signed payload failed signature validation")]
    InvalidSignature,
    #[error("signed payload is expired")]
    Expired,
    #[error("failed to encode payload: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub fn sign_payload<T: Serialize>(signing_secret: &str, value: &T) -> Result<String, SigningError> {
    ensure_signing_secret(signing_secret)?;

    let payload = serde_json::to_vec(value)?;
    let payload_hex = hex::encode(&payload);
    let signature_hex = hex::encode(signing_hmac(signing_secret, &payload));
    Ok(format!("{payload_hex}.{signature_hex}"))
}

pub fn verify_signed_payload<T: DeserializeOwned>(
    signing_secret: &str,
    token: &str,
) -> Result<T, SigningError> {
    ensure_signing_secret(signing_secret)?;

    let (payload_hex, signature_hex) = token.split_once('.').ok_or(SigningError::MalformedToken)?;
    let payload = hex::decode(payload_hex).map_err(|_| SigningError::MalformedToken)?;
    let expected_signature = signing_hmac(signing_secret, &payload);
    let actual_signature = hex::decode(signature_hex).map_err(|_| SigningError::MalformedToken)?;
    if !constant_time_eq(&expected_signature, &actual_signature) {
        return Err(SigningError::InvalidSignature);
    }
    serde_json::from_slice(&payload).map_err(SigningError::from)
}

fn ensure_signing_secret(signing_secret: &str) -> Result<(), SigningError> {
    if signing_secret.is_empty() {
        return Err(SigningError::MissingSigningSecret);
    }
    Ok(())
}

fn signing_hmac(signing_secret: &str, payload: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .expect("sha256 hmac accepts any key length");
    mac.update(payload);
    mac.finalize().into_bytes().to_vec()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0u8;
    for (lhs, rhs) in left.iter().zip(right.iter()) {
        diff |= lhs ^ rhs;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct TestPayload {
        name: String,
        value: i64,
    }

    const SECRET: &str = "test-signing-secret";

    #[test]
    fn sign_and_verify_round_trips() {
        let payload = TestPayload {
            name: "hello".to_string(),
            value: 42,
        };
        let token = sign_payload(SECRET, &payload).expect("should sign");
        let decoded: TestPayload = verify_signed_payload(SECRET, &token).expect("should verify");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn tampering_invalidates_signature() {
        let payload = TestPayload {
            name: "hello".to_string(),
            value: 42,
        };
        let token = sign_payload(SECRET, &payload).expect("should sign");

        let mut tampered = token.clone();
        tampered.pop();
        tampered.push('0');

        assert!(matches!(
            verify_signed_payload::<TestPayload>(SECRET, &tampered),
            Err(SigningError::InvalidSignature)
        ));
    }

    #[test]
    fn wrong_secret_invalidates_signature() {
        let payload = TestPayload {
            name: "hello".to_string(),
            value: 42,
        };
        let token = sign_payload(SECRET, &payload).expect("should sign");
        assert!(matches!(
            verify_signed_payload::<TestPayload>("wrong-secret", &token),
            Err(SigningError::InvalidSignature)
        ));
    }

    #[test]
    fn empty_secret_is_rejected() {
        let payload = TestPayload {
            name: "hello".to_string(),
            value: 42,
        };
        assert!(matches!(
            sign_payload("", &payload),
            Err(SigningError::MissingSigningSecret)
        ));
    }

    #[test]
    fn empty_secret_is_rejected_when_verifying() {
        let payload = TestPayload {
            name: "hello".to_string(),
            value: 42,
        };
        let token = sign_payload(SECRET, &payload).expect("should sign");

        assert!(matches!(
            verify_signed_payload::<TestPayload>("", &token),
            Err(SigningError::MissingSigningSecret)
        ));
    }

    #[test]
    fn malformed_token_is_rejected() {
        assert!(matches!(
            verify_signed_payload::<TestPayload>(SECRET, "not-a-valid-token"),
            Err(SigningError::MalformedToken)
        ));
    }
}
