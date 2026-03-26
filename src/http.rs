use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRequest {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawResponse {
    pub status: u16,
    pub body: String,
}

pub fn form_body(values: &[(&str, &str)]) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[derive(Debug, Error)]
pub enum PercentDecodeError {
    #[error("percent-encoded input contains an incomplete escape sequence at byte {index}")]
    IncompleteEscape { index: usize },
    #[error("percent-encoded input contains an invalid escape sequence at byte {index}")]
    InvalidEscape { index: usize },
    #[error("percent-encoded input is not valid UTF-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

/// Decodes `%XX` escapes in a URL component.
///
/// Unlike `application/x-www-form-urlencoded` decoders, this leaves `+`
/// untouched rather than treating it as a space.
pub fn percent_decode(value: &str) -> Result<String, PercentDecodeError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(PercentDecodeError::IncompleteEscape { index });
            }

            let hi = decode_hex_nibble(bytes[index + 1])
                .ok_or(PercentDecodeError::InvalidEscape { index })?;
            let lo = decode_hex_nibble(bytes[index + 2])
                .ok_or(PercentDecodeError::InvalidEscape { index })?;
            out.push((hi << 4) | lo);
            index += 3;
            continue;
        }

        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(out).map_err(PercentDecodeError::from)
}

fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_leaves_unreserved_chars_unchanged() {
        assert_eq!(
            percent_encode("hello-world_123.test~ok"),
            "hello-world_123.test~ok"
        );
    }

    #[test]
    fn percent_encode_escapes_special_chars() {
        assert_eq!(percent_encode("a b&c=d"), "a%20b%26c%3Dd");
    }

    #[test]
    fn percent_decode_round_trips() {
        let original = "hello world&foo=bar";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded).expect("decoding should succeed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn form_body_joins_encoded_pairs() {
        let body = form_body(&[("grant_type", "authorization_code"), ("code", "abc 123")]);
        assert_eq!(body, "grant_type=authorization_code&code=abc%20123");
    }

    #[test]
    fn percent_decode_round_trips_utf8() {
        let original = "héllo 東京";
        let encoded = percent_encode(original);
        let decoded = percent_decode(&encoded).expect("decoding should succeed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn percent_decode_preserves_plus_signs() {
        assert_eq!(
            percent_decode("keep+plus").expect("decoding should succeed"),
            "keep+plus"
        );
    }

    #[test]
    fn percent_decode_rejects_invalid_escape_sequences() {
        assert!(matches!(
            percent_decode("%zz"),
            Err(PercentDecodeError::InvalidEscape { index: 0 })
        ));
    }

    #[test]
    fn percent_decode_rejects_incomplete_escape_sequences() {
        assert!(matches!(
            percent_decode("%A"),
            Err(PercentDecodeError::IncompleteEscape { index: 0 })
        ));
    }
}
