use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RateLimit {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ResponseMeta {
    pub headers: Vec<ResponseHeader>,
    pub pagination_cursor: Option<String>,
    pub rate_limit: Option<RateLimit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<ResponseHeader>,
    pub body: String,
}

impl HttpResponse {
    pub fn into_raw_response(self) -> RawResponse {
        RawResponse {
            status: self.status,
            body: self.body,
        }
    }

    pub fn meta(&self) -> ResponseMeta {
        let pagination_cursor = header_value(&self.headers, "Ratelimit-Cursor")
            .map(ToOwned::to_owned)
            .or_else(|| pagination_cursor_from_link(&self.headers))
            .or_else(|| pagination_cursor_from_body(&self.body));
        let rate_limit = RateLimit {
            limit: header_value(&self.headers, "Ratelimit-Limit")
                .and_then(|value| value.parse().ok()),
            remaining: header_value(&self.headers, "Ratelimit-Remaining")
                .and_then(|value| value.parse().ok()),
            reset: header_value(&self.headers, "Ratelimit-Reset")
                .and_then(|value| value.parse().ok()),
        };

        ResponseMeta {
            headers: self.headers.clone(),
            pagination_cursor,
            rate_limit: if rate_limit.limit.is_some()
                || rate_limit.remaining.is_some()
                || rate_limit.reset.is_some()
            {
                Some(rate_limit)
            } else {
                None
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedRequestBuilder {
    url: String,
    method: HttpMethod,
    headers: Vec<(String, String)>,
    query: Vec<(String, String)>,
    body: Option<String>,
}

impl PreparedRequestBuilder {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            method,
            headers: Vec::new(),
            query: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn query_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn json_body(mut self, value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        self.headers
            .push(("Content-Type".to_string(), "application/json".to_string()));
        self.body = Some(serde_json::to_string(value)?);
        Ok(self)
    }

    pub fn build(self) -> PreparedRequest {
        PreparedRequest {
            url: append_query_params(&self.url, &self.query),
            method: self.method,
            headers: self.headers,
            body: self.body,
        }
    }
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

pub fn append_query_params(url: &str, params: &[(String, String)]) -> String {
    if params.is_empty() {
        return url.to_string();
    }

    let separator = if url.contains('?') { '&' } else { '?' };
    let query = params
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{url}{separator}{query}")
}

fn header_value<'a>(headers: &'a [ResponseHeader], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

fn pagination_cursor_from_link(headers: &[ResponseHeader]) -> Option<String> {
    let link = header_value(headers, "Link")?;
    let cursor = link.split("cursor=").nth(1)?;
    let cursor = cursor.split(['&', '>']).next()?;
    Some(cursor.to_string())
}

fn pagination_cursor_from_body(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("pagination")
        .and_then(|pagination| pagination.get("cursor"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
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
    fn append_query_params_uses_correct_separator() {
        assert_eq!(
            append_query_params(
                "https://api.twitch.tv/helix/users",
                &[("login".to_string(), "foo".to_string())]
            ),
            "https://api.twitch.tv/helix/users?login=foo"
        );
        assert_eq!(
            append_query_params(
                "https://api.twitch.tv/helix/users?first=1",
                &[("after".to_string(), "cursor".to_string())]
            ),
            "https://api.twitch.tv/helix/users?first=1&after=cursor"
        );
    }

    #[test]
    fn prepared_request_builder_encodes_json_body() {
        let request = PreparedRequestBuilder::new(HttpMethod::Patch, "https://example.com")
            .query_param("first", "1")
            .json_body(&serde_json::json!({"hello": "world"}))
            .expect("json should serialize")
            .build();
        assert_eq!(request.method, HttpMethod::Patch);
        assert_eq!(request.url, "https://example.com?first=1");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "Content-Type" && value == "application/json")
        );
    }

    #[test]
    fn http_response_extracts_meta() {
        let response = HttpResponse {
            status: 200,
            headers: vec![
                ResponseHeader {
                    name: "Ratelimit-Limit".to_string(),
                    value: "800".to_string(),
                },
                ResponseHeader {
                    name: "Ratelimit-Remaining".to_string(),
                    value: "799".to_string(),
                },
            ],
            body: r#"{"pagination":{"cursor":"abc"}}"#.to_string(),
        };
        let meta = response.meta();
        assert_eq!(meta.pagination_cursor.as_deref(), Some("abc"));
        assert_eq!(
            meta.rate_limit
                .expect("rate limit should be parsed")
                .remaining,
            Some(799)
        );
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
