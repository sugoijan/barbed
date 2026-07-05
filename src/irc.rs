use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwitchIrcConfig {
    pub login: String,
    pub token: String,
    pub channel: String,
    pub host: String,
    pub port: u16,
    pub request_capabilities: bool,
    pub raw_mode: bool,
}

impl TwitchIrcConfig {
    pub fn new(
        login: impl Into<String>,
        token: impl Into<String>,
        channel: impl Into<String>,
    ) -> Self {
        Self {
            login: login.into(),
            token: token.into(),
            channel: channel.into(),
            host: "irc.chat.twitch.tv".to_string(),
            port: 6667,
            request_capabilities: true,
            raw_mode: false,
        }
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_request_capabilities(mut self, request_capabilities: bool) -> Self {
        self.request_capabilities = request_capabilities;
        self
    }

    pub fn with_raw_mode(mut self, raw_mode: bool) -> Self {
        self.raw_mode = raw_mode;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwitchIrcPrivmsg {
    pub channel: String,
    pub login: String,
    pub display_name: Option<String>,
    pub badges: Vec<String>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TwitchIrcEvent {
    Privmsg(TwitchIrcPrivmsg),
    Notice(String),
    Ping { payload: Option<String> },
    Raw(String),
    Other(String),
}

#[derive(Debug, Error)]
pub enum IrcError {
    #[error("twitch IRC I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("twitch IRC client is already closed")]
    Closed,
}

#[async_trait]
pub trait TwitchIrcApi: Send {
    async fn next_event(&mut self) -> Result<Option<TwitchIrcEvent>, IrcError>;

    async fn send_raw(&mut self, line: &str) -> Result<(), IrcError>;

    async fn close(&mut self) -> Result<(), IrcError>;
}

pub fn normalize_token(token: &str) -> String {
    if token.len() >= 6 && token[..6].eq_ignore_ascii_case("oauth:") {
        format!("oauth:{}", &token[6..])
    } else {
        format!("oauth:{token}")
    }
}

pub fn connect_commands(config: &TwitchIrcConfig) -> Vec<String> {
    let mut commands = vec![
        format!("PASS {}", normalize_token(&config.token)),
        format!("NICK {}", config.login.to_lowercase()),
    ];
    if config.request_capabilities {
        commands
            .push("CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership".to_string());
    }
    commands.push(format!("JOIN #{}", config.channel.to_lowercase()));
    commands
}

pub fn disconnect_commands(config: &TwitchIrcConfig) -> Vec<String> {
    vec![
        format!("PART #{}", config.channel.to_lowercase()),
        "QUIT".to_string(),
    ]
}

pub fn ping_response(line: &str) -> Option<String> {
    let payload = line.strip_prefix("PING")?.trim_start();
    if payload.is_empty() {
        Some("PONG".to_string())
    } else {
        Some(format!("PONG {payload}"))
    }
}

pub fn unescape_irc_tag(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    's' => result.push(' '),
                    ':' => result.push(';'),
                    'r' => result.push('\r'),
                    'n' => result.push('\n'),
                    '\\' => result.push('\\'),
                    other => {
                        result.push('\\');
                        result.push(other);
                    }
                }
            } else {
                result.push('\\');
            }
        } else {
            result.push(ch);
        }
    }
    result
}

pub fn split_tags(line: &str) -> Option<(Option<&str>, &str)> {
    if let Some(stripped) = line.strip_prefix('@') {
        let space = stripped.find(' ')?;
        let tags = &stripped[..space];
        let remainder = stripped[space + 1..].trim_start();
        Some((Some(tags), remainder))
    } else {
        Some((None, line.trim_start()))
    }
}

pub fn parse_privmsg(line: &str) -> Option<TwitchIrcPrivmsg> {
    let (tags_section, mut remainder) = split_tags(line)?;

    if !remainder.starts_with(':') {
        return None;
    }

    let prefix_end = remainder.find(' ')?;
    let prefix = &remainder[1..prefix_end];
    remainder = remainder[prefix_end + 1..].trim_start();

    if !remainder.starts_with("PRIVMSG ") {
        return None;
    }

    remainder = &remainder["PRIVMSG ".len()..];
    let message_sep = remainder.find(" :")?;
    let channel = remainder[..message_sep]
        .trim()
        .trim_start_matches('#')
        .to_string();
    let text = remainder[message_sep + 2..].to_string();

    let login = prefix.split('!').next().unwrap_or(prefix).to_string();
    let mut display_name = None;
    let mut badges = Vec::new();

    if let Some(tags) = tags_section {
        for tag in tags.split(';') {
            let mut parts = tag.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default();
            match key {
                "display-name" if !value.is_empty() => {
                    display_name = Some(unescape_irc_tag(value));
                }
                "badges" if !value.is_empty() => {
                    badges = value
                        .split(',')
                        .filter(|entry| !entry.is_empty())
                        .map(ToOwned::to_owned)
                        .collect();
                }
                _ => {}
            }
        }
    }

    Some(TwitchIrcPrivmsg {
        channel,
        login,
        display_name,
        badges,
        text,
    })
}

pub fn parse_notice(line: &str) -> Option<String> {
    let (_, mut remainder) = split_tags(line)?;

    if remainder.starts_with(':') {
        let prefix_end = remainder.find(' ')?;
        remainder = remainder[prefix_end + 1..].trim_start();
    }

    if !remainder.starts_with("NOTICE ") {
        return None;
    }

    remainder = &remainder["NOTICE ".len()..];
    let text = remainder.split_once(" :")?.1;
    Some(unescape_irc_tag(text))
}

pub fn parse_line(line: &str, raw_mode: bool) -> TwitchIrcEvent {
    if let Some(payload) = line.strip_prefix("PING") {
        let payload = payload.trim_start();
        return TwitchIrcEvent::Ping {
            payload: if payload.is_empty() {
                None
            } else {
                Some(payload.to_string())
            },
        };
    }

    if raw_mode {
        return TwitchIrcEvent::Raw(line.to_string());
    }

    if let Some(message) = parse_privmsg(line) {
        TwitchIrcEvent::Privmsg(message)
    } else if let Some(notice) = parse_notice(line) {
        TwitchIrcEvent::Notice(notice)
    } else {
        TwitchIrcEvent::Other(line.to_string())
    }
}

#[derive(Clone, Default)]
struct InMemoryState {
    inbound: VecDeque<String>,
    outbound: Vec<String>,
    closed: bool,
}

#[derive(Clone)]
pub struct InMemoryTwitchIrcClient {
    config: TwitchIrcConfig,
    state: Arc<Mutex<InMemoryState>>,
}

impl InMemoryTwitchIrcClient {
    pub fn new(config: TwitchIrcConfig) -> Self {
        let state = InMemoryState {
            inbound: VecDeque::new(),
            outbound: connect_commands(&config),
            closed: false,
        };
        Self {
            config,
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn push_inbound_line(&self, line: impl Into<String>) {
        self.state
            .lock()
            .expect("in-memory IRC client lock poisoned")
            .inbound
            .push_back(line.into());
    }

    pub fn outbound_lines(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("in-memory IRC client lock poisoned")
            .outbound
            .clone()
    }
}

#[async_trait]
impl TwitchIrcApi for InMemoryTwitchIrcClient {
    async fn next_event(&mut self) -> Result<Option<TwitchIrcEvent>, IrcError> {
        let mut state = self
            .state
            .lock()
            .expect("in-memory IRC client lock poisoned");
        let Some(line) = state.inbound.pop_front() else {
            return Ok(None);
        };
        if let Some(response) = ping_response(&line) {
            state.outbound.push(response);
        }
        Ok(Some(parse_line(&line, self.config.raw_mode)))
    }

    async fn send_raw(&mut self, line: &str) -> Result<(), IrcError> {
        let mut state = self
            .state
            .lock()
            .expect("in-memory IRC client lock poisoned");
        if state.closed {
            return Err(IrcError::Closed);
        }
        state.outbound.push(line.to_string());
        Ok(())
    }

    async fn close(&mut self) -> Result<(), IrcError> {
        let mut state = self
            .state
            .lock()
            .expect("in-memory IRC client lock poisoned");
        if state.closed {
            return Err(IrcError::Closed);
        }
        state.outbound.extend(disconnect_commands(&self.config));
        state.closed = true;
        Ok(())
    }
}

#[cfg(feature = "tokio-irc")]
pub struct TwitchIrcClient {
    config: TwitchIrcConfig,
    reader: tokio::io::BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
    closed: bool,
}

#[cfg(feature = "tokio-irc")]
impl TwitchIrcClient {
    pub async fn connect(config: TwitchIrcConfig) -> Result<Self, IrcError> {
        use tokio::net::TcpStream;

        let address = format!("{}:{}", config.host, config.port);
        let stream = TcpStream::connect(&address).await?;
        let (read_half, mut write_half) = stream.into_split();

        for command in connect_commands(&config) {
            send_line(&mut write_half, &command).await?;
        }

        Ok(Self {
            config,
            reader: tokio::io::BufReader::new(read_half),
            writer: write_half,
            closed: false,
        })
    }
}

#[cfg(feature = "tokio-irc")]
#[async_trait]
impl TwitchIrcApi for TwitchIrcClient {
    async fn next_event(&mut self) -> Result<Option<TwitchIrcEvent>, IrcError> {
        use tokio::io::AsyncBufReadExt;

        if self.closed {
            return Ok(None);
        }

        let mut buffer = String::new();
        loop {
            buffer.clear();
            let bytes = self.reader.read_line(&mut buffer).await?;
            if bytes == 0 {
                return Ok(None);
            }

            let line = buffer.trim_end_matches(&['\r', '\n'][..]).to_string();
            if line.is_empty() {
                continue;
            }

            if let Some(response) = ping_response(&line) {
                send_line(&mut self.writer, &response).await?;
            }
            return Ok(Some(parse_line(&line, self.config.raw_mode)));
        }
    }

    async fn send_raw(&mut self, line: &str) -> Result<(), IrcError> {
        if self.closed {
            return Err(IrcError::Closed);
        }
        send_line(&mut self.writer, line).await
    }

    async fn close(&mut self) -> Result<(), IrcError> {
        use tokio::io::AsyncWriteExt;

        if self.closed {
            return Err(IrcError::Closed);
        }
        for command in disconnect_commands(&self.config) {
            send_line(&mut self.writer, &command).await?;
        }
        self.writer.shutdown().await?;
        self.closed = true;
        Ok(())
    }
}

#[cfg(feature = "tokio-irc")]
async fn send_line(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    line: &str,
) -> Result<(), IrcError> {
    use tokio::io::AsyncWriteExt;

    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\r\n").await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_commands_match_twitch_cli_sequence() {
        let config = TwitchIrcConfig::new("Viewer", "secret", "CoolChannel");
        assert_eq!(
            connect_commands(&config),
            vec![
                "PASS oauth:secret".to_string(),
                "NICK viewer".to_string(),
                "CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership".to_string(),
                "JOIN #coolchannel".to_string(),
            ]
        );
    }

    #[test]
    fn parse_privmsg_extracts_tags_and_payload() {
        let line = "@badges=moderator/1,subscriber/6;display-name=Viewer\\sName :viewer!viewer@viewer.tmi.twitch.tv PRIVMSG #coolchannel :hello there";
        let message = parse_privmsg(line).expect("privmsg should parse");
        assert_eq!(message.channel, "coolchannel");
        assert_eq!(message.login, "viewer");
        assert_eq!(message.display_name.as_deref(), Some("Viewer Name"));
        assert_eq!(message.badges, vec!["moderator/1", "subscriber/6"]);
        assert_eq!(message.text, "hello there");
    }

    #[test]
    fn parse_notice_extracts_body() {
        let line = ":tmi.twitch.tv NOTICE #coolchannel :Login authentication failed";
        assert_eq!(
            parse_notice(line).expect("notice should parse"),
            "Login authentication failed"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn in_memory_client_records_pong_and_shutdown() {
        let config = TwitchIrcConfig::new("viewer", "secret", "coolchannel");
        let mut client = InMemoryTwitchIrcClient::new(config);
        client.push_inbound_line("PING :tmi.twitch.tv");

        let event = client
            .next_event()
            .await
            .expect("event should succeed")
            .expect("event should exist");
        assert_eq!(
            event,
            TwitchIrcEvent::Ping {
                payload: Some(":tmi.twitch.tv".to_string())
            }
        );

        client.close().await.expect("close should succeed");
        let outbound = client.outbound_lines();
        assert!(outbound.contains(&"PONG :tmi.twitch.tv".to_string()));
        assert!(outbound.contains(&"QUIT".to_string()));
    }

    #[test]
    fn raw_mode_preserves_original_line() {
        let event = parse_line(":tmi.twitch.tv ROOMSTATE #coolchannel", true);
        assert_eq!(
            event,
            TwitchIrcEvent::Raw(":tmi.twitch.tv ROOMSTATE #coolchannel".to_string())
        );
    }
}
