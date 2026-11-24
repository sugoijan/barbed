//! A `reqwest` client that makes a [`SetLoader`]'s requests.

use futures_util::future::join_all;

use crate::load::{LoadError, SetLoader};

/// `User-Agent` this client sends; tracks the crate version.
pub const USER_AGENT: &str = concat!("barbed-emotes/", env!("CARGO_PKG_VERSION"));

/// Reads emote sets over HTTP for programs that have `reqwest` and an async
/// runtime. Everything it does is [`SetLoader`]'s: this only carries the
/// requests.
#[derive(Clone, Debug)]
pub struct EmoteClient {
    http: reqwest::Client,
}

impl EmoteClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder().user_agent(USER_AGENT).build()?,
        })
    }

    /// Uses `http` for its requests: its timeouts, its proxy, its headers.
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

    /// Makes the requests `loader` asks for, each round at once, until it is
    /// done.
    pub async fn load(&self, loader: &mut SetLoader) {
        loop {
            let requests = loader.requests();
            if requests.is_empty() {
                return;
            }
            let answers = join_all(requests.iter().map(|request| self.get(&request.url))).await;
            for (request, answer) in requests.iter().zip(answers) {
                let answer = answer.as_deref().map_err(Clone::clone);
                loader.answered(request.key, request.via, answer);
            }
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
