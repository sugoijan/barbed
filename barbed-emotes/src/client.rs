//! A `reqwest` client that makes a [`SetLoader`]'s requests.

use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use crate::load::{LoadError, SetLoader};

/// `User-Agent` this client sends; tracks the crate version.
pub const USER_AGENT: &str = concat!("barbed-emotes/", env!("CARGO_PKG_VERSION"));

/// How long [`EmoteClient::new`] waits for one answer. A provider that hangs
/// then counts as failed, and the chat goes on with the other sets.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// Reads emote sets over HTTP for programs that have `reqwest` and an async
/// runtime. Everything it does is [`SetLoader`]'s: this only carries the
/// requests.
#[derive(Clone, Debug)]
pub struct EmoteClient {
    http: reqwest::Client,
}

impl EmoteClient {
    /// A client that sends [`USER_AGENT`] and gives each request
    /// [`DEFAULT_TIMEOUT`].
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(DEFAULT_TIMEOUT)
                .build()?,
        })
    }

    /// Uses `http` for its requests: its timeouts, its proxy, its headers.
    /// A client without a timeout can keep [`load`](Self::load) waiting for
    /// as long as a provider does not answer.
    pub fn with_http(http: reqwest::Client) -> Self {
        Self { http }
    }

    async fn get(&self, url: &str) -> Result<String, LoadError> {
        let failed = |error: reqwest::Error| LoadError::Failed(error.to_string());
        let response = self.http.get(url).send().await.map_err(failed)?;
        let status = response.status();
        if !status.is_success() {
            return Err(LoadError::Status(status.as_u16()));
        }
        response.text().await.map_err(failed)
    }

    /// Makes the requests `loader` asks for, all at once, and hands it each
    /// answer as it comes, until it is done: a small set is read without
    /// waiting for a large one, and a retry starts as soon as its first
    /// request failed.
    pub async fn load(&self, loader: &mut SetLoader) {
        let mut pending = FuturesUnordered::new();
        loop {
            for request in loader.requests() {
                pending.push(async move {
                    let answer = self.get(&request.url).await;
                    (request, answer)
                });
            }
            let Some((request, answer)) = pending.next().await else {
                return;
            };
            let answer = answer.as_deref().map_err(Clone::clone);
            loader.answered(request.key, request.via, answer);
        }
    }

    /// The sets of the channel of Twitch user `twitch_user_id` and the global
    /// ones ([`SetLoader::for_channel`]), read. What failed is in the
    /// loader's [`report`](SetLoader::report).
    pub async fn channel(&self, twitch_user_id: &str) -> SetLoader {
        let mut loader = SetLoader::for_channel(twitch_user_id);
        self.load(&mut loader).await;
        loader
    }
}
