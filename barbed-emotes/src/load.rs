//! Reading a chat's emote sets, without doing the requests.
//!
//! A [`SetLoader`] knows which sets a chat needs and where each is read. The
//! caller makes the HTTP requests it asks for ([`SetLoader::requests`]) with
//! whatever client it has (a browser's `fetch`, a thread, an async client)
//! and hands every answer back ([`SetLoader::answered`]). The loader parses
//! them, falls back to a proxy where a direct request failed, and says what
//! happened to each set ([`SetLoader::report`]).

use std::fmt;
use std::sync::Arc;

use crate::index::{DEFAULT_PROVIDER_ORDER, EmoteIndex, EmoteScope};
use crate::model::{Emote, EmoteProvider, EmoteSet, ParseError};
use crate::{bttv, ffz, proxy, seventv, twitch};

/// How a request reaches a provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Via {
    /// Straight to the provider.
    Direct,
    /// Through the caller's own server ([`crate::proxy`]).
    Proxy,
}

impl fmt::Display for Via {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Direct => "direct",
            Self::Proxy => "proxy",
        })
    }
}

/// A request for the caller to make: a `GET` of `url`, its body handed to
/// [`SetLoader::answered`] with the same `key` and `via`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetRequest {
    pub key: usize,
    pub provider: EmoteProvider,
    pub scope: EmoteScope,
    pub url: String,
    pub via: Via,
}

/// Why a request gave no body.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// The server answered with a status that is not a success.
    #[error("HTTP {0}")]
    Status(u16),
    /// The request was refused, blocked, cut or timed out.
    #[error("{0}")]
    Failed(String),
}

/// Where a set's read stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SetStatus {
    /// To be asked for by the next [`SetLoader::requests`].
    Waiting,
    /// Asked for, not answered yet.
    Asked,
    Read,
    /// The channel has no set with this provider (its `404`). Not a failure.
    NoSet,
    Failed,
}

impl fmt::Display for SetStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Waiting => "waiting",
            Self::Asked => "asked",
            Self::Read => "read",
            Self::NoSet => "no set",
            Self::Failed => "failed",
        })
    }
}

/// What became of one set, for a log or a debug view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetReport {
    pub provider: EmoteProvider,
    pub scope: EmoteScope,
    pub status: SetStatus,
    /// The way it is being or was read.
    pub via: Option<Via>,
    /// Why each way failed, in turn.
    pub failures: Vec<String>,
    /// Emotes read.
    pub emotes: usize,
    /// How many of them are animated.
    pub animated: usize,
    /// Entries of the answer left out ([`crate::EmoteSet::skipped`]).
    pub skipped: usize,
}

impl fmt::Display for SetReport {
    /// One line: `7tv channel: read direct (42 emotes, 18 animated, 0
    /// skipped)`, with the failures after it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scope = match self.scope {
            EmoteScope::Global => "global",
            EmoteScope::Channel => "channel",
        };
        write!(f, "{} {scope}: {}", self.provider, self.status)?;
        if let Some(via) = self.via {
            write!(f, " {via}")?;
        }
        write!(
            f,
            " ({} emotes, {} animated, {} skipped)",
            self.emotes, self.animated, self.skipped
        )?;
        for failure in &self.failures {
            write!(f, "; {failure}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Waiting(Via),
    Asked(Via),
    Read(Via),
    NoSet,
    Failed,
}

/// How a set's answer is read: its emotes, and how many of its entries were
/// left out.
type Read = fn(&str) -> Result<(Vec<Emote>, usize), ParseError>;

#[derive(Clone, Debug)]
struct Slot {
    provider: EmoteProvider,
    scope: EmoteScope,
    /// The provider's address.
    url: String,
    read: Read,
    state: State,
    failures: Vec<String>,
    emotes: Vec<Arc<Emote>>,
    skipped: usize,
}

impl Slot {
    /// The set of `provider` and `scope`, a channel's being that of Twitch
    /// user `channel`: where it is, whether it may be asked directly (no
    /// token needed), and how its answer is read. `None` where there is no
    /// such set, or no address for it.
    fn of(provider: &EmoteProvider, scope: EmoteScope, channel: &str) -> Option<Self> {
        use EmoteProvider::{BetterTtv, FrankerFaceZ, SevenTv, Twitch};
        use EmoteScope::{Channel, Global};

        fn whole(set: EmoteSet) -> (Vec<Emote>, usize) {
            (set.emotes, set.skipped)
        }
        let (url, direct, read): (String, bool, Read) = match (provider, scope) {
            (Twitch, Global) => (twitch::GLOBAL_EMOTES_URL.to_string(), false, |body| {
                Ok(whole(twitch::parse_emotes(body)?))
            }),
            (SevenTv, Channel) => (seventv::user_url(channel)?, true, |body| {
                Ok(whole(
                    seventv::parse_user(body)?.emote_set.unwrap_or_default(),
                ))
            }),
            (SevenTv, Global) => (seventv::GLOBAL_SET_URL.to_string(), true, |body| {
                Ok(whole(seventv::parse_emote_set(body)?))
            }),
            (BetterTtv, Channel) => (bttv::channel_url(channel)?, true, |body| {
                let channel = bttv::parse_channel(body)?;
                let skipped = channel.skipped();
                Ok((channel.into_emotes(), skipped))
            }),
            (BetterTtv, Global) => (bttv::GLOBAL_URL.to_string(), true, |body| {
                Ok(whole(bttv::parse_global(body)?))
            }),
            (FrankerFaceZ, Channel) => (ffz::room_url(channel)?, true, |body| {
                let room = ffz::parse_room(body)?;
                let skipped = room.skipped();
                Ok((room.into_emotes(), skipped))
            }),
            (FrankerFaceZ, Global) => (ffz::GLOBAL_URL.to_string(), true, |body| {
                let global = ffz::parse_global(body)?;
                Ok((global.default_emotes(), global.skipped()))
            }),
            _ => return None,
        };
        Some(Self {
            provider: provider.clone(),
            scope,
            url,
            read,
            state: State::Waiting(if direct { Via::Direct } else { Via::Proxy }),
            failures: Vec::new(),
            emotes: Vec::new(),
            skipped: 0,
        })
    }
}

/// The emote sets of one chat, read through requests the caller makes.
///
/// ```
/// use barbed_emotes::{LoadError, SetLoader};
///
/// let mut loader = SetLoader::for_channel("1971641")
///     .with_proxy("https://example.test/api/emotes");
/// while !loader.is_done() {
///     for request in loader.requests() {
///         // GET request.url with your HTTP client…
///         let answer: Result<&str, LoadError> = Err(LoadError::Status(404));
///         loader.answered(request.key, request.via, answer);
///     }
/// }
/// let index = loader.index();
/// # assert!(index.is_empty());
/// ```
#[derive(Clone, Debug, Default)]
pub struct SetLoader {
    slots: Vec<Slot>,
    proxy: Option<String>,
    version: u64,
}

impl SetLoader {
    /// 7TV's, BetterTTV's and FrankerFaceZ's sets for the channel of Twitch
    /// user `twitch_user_id`, each followed by the provider's global set.
    ///
    /// The channel's sets are left out where the id is not of Twitch's shape
    /// ([`is_plain_id`](crate::is_plain_id)).
    pub fn for_channel(twitch_user_id: &str) -> Self {
        let mut loader = Self::default();
        for provider in &DEFAULT_PROVIDER_ORDER {
            for scope in [EmoteScope::Channel, EmoteScope::Global] {
                loader
                    .slots
                    .extend(Slot::of(provider, scope, twitch_user_id));
            }
        }
        loader
    }

    /// 7TV's, BetterTTV's and FrankerFaceZ's global sets alone.
    pub fn for_globals() -> Self {
        let mut loader = Self::default();
        for provider in &DEFAULT_PROVIDER_ORDER {
            loader
                .slots
                .extend(Slot::of(provider, EmoteScope::Global, ""));
        }
        loader
    }

    /// Also reads Twitch's global emotes, first of all. Helix needs a token,
    /// so they are asked through the proxy alone and fail without one
    /// ([`with_proxy`](Self::with_proxy)).
    ///
    /// A real chat does not need them: its messages name their Twitch emotes.
    /// They are for showing emotes outside a message, a picker or a demo.
    pub fn with_twitch_globals(mut self) -> Self {
        let twitch = Slot::of(&EmoteProvider::Twitch, EmoteScope::Global, "");
        self.slots.splice(0..0, twitch);
        self
    }

    /// Keeps only the sets of `providers`.
    pub fn only(mut self, providers: &[EmoteProvider]) -> Self {
        self.slots.retain(|slot| providers.contains(&slot.provider));
        self
    }

    /// Where the caller's server mounts its emote proxy
    /// (`https://example.test/api/emotes`): a set whose direct request failed
    /// is asked once more as `<base>/<route>` ([`proxy::route`]).
    pub fn with_proxy(mut self, base: impl Into<String>) -> Self {
        self.proxy = Some(base.into().trim_end_matches('/').to_string());
        self
    }

    /// The proxy's base, as given to [`with_proxy`](Self::with_proxy).
    pub fn proxy(&self) -> Option<&str> {
        self.proxy.as_deref()
    }

    /// The requests to make now: every set once, and once more through the
    /// proxy after its direct request failed. Empty when there is nothing to
    /// ask (which is not [`is_done`](Self::is_done) while answers are due).
    pub fn requests(&mut self) -> Vec<SetRequest> {
        let mut requests = Vec::new();
        for (key, slot) in self.slots.iter_mut().enumerate() {
            let State::Waiting(via) = slot.state else {
                continue;
            };
            let url = match via {
                Via::Direct => Some(slot.url.clone()),
                Via::Proxy => self
                    .proxy
                    .as_deref()
                    .and_then(|base| proxy::proxied(base, &slot.url)),
            };
            match url {
                Some(url) => {
                    slot.state = State::Asked(via);
                    requests.push(SetRequest {
                        key,
                        provider: slot.provider.clone(),
                        scope: slot.scope,
                        url,
                        via,
                    });
                }
                None => {
                    slot.failures.push("no proxy to ask through".to_string());
                    slot.state = State::Failed;
                }
            }
        }
        requests
    }

    /// The answer to request `key` made `via`: its body, or why there is
    /// none. Whether a set was read (the [`index`](Self::index) changed).
    ///
    /// - A body its parser cannot read is a failure, and so is a global set
    ///   without a single emote (a channel's may be empty).
    /// - A channel's `404` means it has no set with that provider: no
    ///   failure, and the proxy is not asked.
    /// - Any other failure of a direct request is asked again through the
    ///   proxy, where there is one.
    /// - An answer to a request that is not due (asked twice, another `via`)
    ///   is ignored.
    pub fn answered(&mut self, key: usize, via: Via, answer: Result<&str, LoadError>) -> bool {
        let has_proxy = self.proxy.is_some();
        let Some(slot) = self.slots.get_mut(key) else {
            return false;
        };
        if slot.state != State::Asked(via) {
            return false;
        }
        let read = match answer {
            Err(error @ LoadError::Status(404)) if slot.scope == EmoteScope::Channel => {
                slot.failures
                    .push(format!("the channel has no set ({error})"));
                slot.state = State::NoSet;
                return false;
            }
            Err(error) => Err(error.to_string()),
            Ok(body) => match (slot.read)(body) {
                Err(error) => Err(error.to_string()),
                Ok((emotes, skipped)) if emotes.is_empty() && slot.scope == EmoteScope::Global => {
                    Err(format!(
                        "no emote in {} bytes ({skipped} entries skipped)",
                        body.len()
                    ))
                }
                Ok(read) => Ok(read),
            },
        };
        match read {
            Ok((emotes, skipped)) => {
                slot.emotes = emotes.into_iter().map(Arc::new).collect();
                slot.skipped = skipped;
                slot.state = State::Read(via);
                self.version += 1;
                true
            }
            Err(why) => {
                let way = match via {
                    Via::Direct => "directly",
                    Via::Proxy => "through the proxy",
                };
                slot.failures.push(format!("{way}: {why}"));
                let retry = via == Via::Direct && has_proxy && proxy::route(&slot.url).is_some();
                slot.state = if retry {
                    State::Waiting(Via::Proxy)
                } else {
                    State::Failed
                };
                false
            }
        }
    }

    /// Whether every set was read or given up on: nothing is waiting or
    /// asked.
    pub fn is_done(&self) -> bool {
        self.slots
            .iter()
            .all(|slot| !matches!(slot.state, State::Waiting(_) | State::Asked(_)))
    }

    /// A number that grows whenever a set was read: when it changed, take
    /// the [`index`](Self::index) again.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Every emote of the sets read so far, to match in messages. A chat can
    /// start with what has arrived and take the index again as more does.
    pub fn index(&self) -> EmoteIndex {
        self.index_where(|_| true)
    }

    /// As [`index`](Self::index), with only the emotes `keep` accepts. For a
    /// display that draws no modifiers, `|emote| emote.is_standalone()`
    /// leaves their words as text.
    pub fn index_where(&self, keep: impl Fn(&Emote) -> bool) -> EmoteIndex {
        let mut index = EmoteIndex::new();
        for (scope, _, emotes) in self.sets() {
            for emote in emotes.iter().filter(|emote| keep(emote)) {
                index.insert(scope, Arc::clone(emote));
            }
        }
        index
    }

    /// The sets read so far, in the order they are asked for.
    pub fn sets(&self) -> impl Iterator<Item = (EmoteScope, &EmoteProvider, &[Arc<Emote>])> {
        self.slots
            .iter()
            .filter(|slot| matches!(slot.state, State::Read(_)))
            .map(|slot| (slot.scope, &slot.provider, slot.emotes.as_slice()))
    }

    /// What became of each set, in the order they are asked for.
    pub fn report(&self) -> Vec<SetReport> {
        self.slots
            .iter()
            .map(|slot| {
                let (status, via) = match slot.state {
                    State::Waiting(via) => (SetStatus::Waiting, Some(via)),
                    State::Asked(via) => (SetStatus::Asked, Some(via)),
                    State::Read(via) => (SetStatus::Read, Some(via)),
                    State::NoSet => (SetStatus::NoSet, None),
                    State::Failed => (SetStatus::Failed, None),
                };
                SetReport {
                    provider: slot.provider.clone(),
                    scope: slot.scope,
                    status,
                    via,
                    failures: slot.failures.clone(),
                    emotes: slot.emotes.len(),
                    animated: slot.emotes.iter().filter(|emote| emote.animated).count(),
                    skipped: slot.skipped,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(request: &SetRequest) -> &'static str {
        match (&request.provider, request.scope) {
            (EmoteProvider::Twitch, _) => include_str!("../tests/fixtures/twitch-global.json"),
            (EmoteProvider::SevenTv, EmoteScope::Channel) => {
                include_str!("../tests/fixtures/7tv-user.json")
            }
            (EmoteProvider::SevenTv, EmoteScope::Global) => {
                include_str!("../tests/fixtures/7tv-global.json")
            }
            (EmoteProvider::BetterTtv, EmoteScope::Channel) => {
                include_str!("../tests/fixtures/bttv-user.json")
            }
            (EmoteProvider::BetterTtv, EmoteScope::Global) => {
                include_str!("../tests/fixtures/bttv-global.json")
            }
            (_, EmoteScope::Channel) => include_str!("../tests/fixtures/ffz-room.json"),
            (_, EmoteScope::Global) => include_str!("../tests/fixtures/ffz-global.json"),
        }
    }

    fn failed(why: &str) -> Result<&'static str, LoadError> {
        Err(LoadError::Failed(why.to_string()))
    }

    #[test]
    fn a_channels_and_the_global_sets_are_each_asked_once() {
        let mut loader =
            SetLoader::for_channel("141981764").with_proxy("https://api.test/x/emotes/");
        assert_eq!(loader.proxy(), Some("https://api.test/x/emotes"));
        let requests = loader.requests();
        let urls: Vec<&str> = requests.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://7tv.io/v3/users/twitch/141981764",
                "https://7tv.io/v3/emote-sets/global",
                "https://api.betterttv.net/3/cached/users/twitch/141981764",
                "https://api.betterttv.net/3/cached/emotes/global",
                "https://api.frankerfacez.com/v1/room/id/141981764",
                "https://api.frankerfacez.com/v1/set/global",
            ]
        );
        assert!(requests.iter().all(|r| r.via == Via::Direct));
        assert!(loader.requests().is_empty());
        assert!(!loader.is_done());
        assert!(loader.index().is_empty());
        let before = loader.version();
        for request in &requests {
            assert!(loader.answered(request.key, request.via, Ok(fixture(request))));
        }
        assert!(loader.is_done());
        assert_eq!(loader.version(), before + 6);
        assert_eq!(loader.sets().count(), 6);
        let report = loader.report();
        for set in &report {
            assert_eq!(set.status, SetStatus::Read, "{set}");
            assert_eq!(set.via, Some(Via::Direct));
            assert!(set.emotes > 0, "{set}");
        }
        assert_eq!(
            report[0].to_string(),
            "7tv channel: read direct (2 emotes, 1 animated, 0 skipped)"
        );
        assert_eq!(
            report[3].to_string(),
            "bttv global: read direct (3 emotes, 0 animated, 1 skipped)"
        );
        // The channel's 7TV catJAM over BetterTTV's and the global one.
        let index = loader.index();
        let cat = index.get("catJAM").expect("an emote");
        assert_eq!(cat.id.to_string(), "7tv:60ae7316f7c927fad14e6ca2");
        assert_eq!(
            index.get("PogBTTV").unwrap().id.provider,
            EmoteProvider::BetterTtv
        );
        assert_eq!(
            index.get("LilZ").unwrap().id.provider,
            EmoteProvider::FrankerFaceZ
        );
        assert!(index.get("ZeroCover").is_some() && index.get("w!").is_some());
        // Without what is not drawn by itself.
        let alone = loader.index_where(|emote| emote.is_standalone() && !emote.hidden);
        assert!(alone.get("ZeroCover").is_none() && alone.get("w!").is_none());
        assert!(alone.get("ffzHyper").is_none() && alone.get("HiddenOne").is_none());
        assert_eq!(alone.len(), index.len() - 4);
    }

    #[test]
    fn a_missing_channel_set_is_no_failure_and_a_failure_goes_through_the_proxy() {
        let mut loader = SetLoader::for_channel("9").with_proxy("https://api.test/emotes");
        for request in loader.requests() {
            let answer = match (&request.provider, request.scope) {
                (EmoteProvider::SevenTv, EmoteScope::Channel) => Err(LoadError::Status(404)),
                (EmoteProvider::BetterTtv, EmoteScope::Global) => {
                    failed("TypeError: Failed to fetch")
                }
                (EmoteProvider::FrankerFaceZ, EmoteScope::Global) => Ok("<html>blocked</html>"),
                _ => Ok(fixture(&request)),
            };
            loader.answered(request.key, request.via, answer);
        }
        assert!(!loader.is_done());
        let again = loader.requests();
        let urls: Vec<&str> = again.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://api.test/emotes/bttv-api/3/cached/emotes/global",
                "https://api.test/emotes/ffz-api/v1/set/global",
            ]
        );
        assert!(again.iter().all(|r| r.via == Via::Proxy));
        // An answer that is not due changes nothing.
        assert!(!loader.answered(again[0].key, Via::Direct, Ok(fixture(&again[0]))));
        assert!(!loader.answered(99, Via::Proxy, Ok("")));
        assert!(loader.answered(again[0].key, again[0].via, Ok(fixture(&again[0]))));
        assert!(!loader.answered(again[1].key, again[1].via, Err(LoadError::Status(502))));
        assert!(loader.is_done());
        let report = loader.report();
        assert_eq!(report[0].status, SetStatus::NoSet);
        assert_eq!(
            report[0].to_string(),
            "7tv channel: no set (0 emotes, 0 animated, 0 skipped); the channel has no set (HTTP 404)"
        );
        assert_eq!(
            (report[3].status, report[3].via),
            (SetStatus::Read, Some(Via::Proxy))
        );
        assert_eq!(report[3].failures, ["directly: TypeError: Failed to fetch"]);
        assert_eq!(report[5].status, SetStatus::Failed);
        assert_eq!(report[5].failures.len(), 2);
        assert!(
            report[5].failures[0].starts_with("directly: not JSON"),
            "{}",
            report[5]
        );
        assert_eq!(report[5].failures[1], "through the proxy: HTTP 502");
        assert!(loader.requests().is_empty());
    }

    #[test]
    fn without_a_proxy_a_failure_is_final_and_a_global_set_is_never_empty() {
        let mut loader = SetLoader::for_globals();
        let requests = loader.requests();
        assert_eq!(requests.len(), 3);
        assert!(!loader.answered(0, Via::Direct, failed("offline")));
        // A global set no emote was read from, a channel's may be empty.
        assert!(!loader.answered(1, Via::Direct, Ok("[]")));
        assert!(!loader.answered(2, Via::Direct, Ok(r#"{"default_sets":[],"sets":{}}"#)));
        assert!(loader.is_done());
        let report = loader.report();
        assert!(report.iter().all(|set| set.status == SetStatus::Failed));
        assert_eq!(
            report[1].failures,
            ["directly: no emote in 2 bytes (0 entries skipped)"]
        );
        let mut loader = SetLoader::for_channel("9").only(&[EmoteProvider::BetterTtv]);
        let requests = loader.requests();
        assert_eq!(requests.len(), 2);
        assert!(loader.answered(
            0,
            Via::Direct,
            Ok(r#"{"channelEmotes":[],"sharedEmotes":[]}"#)
        ));
        assert_eq!(loader.report()[0].status, SetStatus::Read);
    }

    #[test]
    fn twitchs_globals_are_asked_through_the_proxy_alone() {
        let mut loader = SetLoader::for_globals()
            .with_twitch_globals()
            .only(&[EmoteProvider::Twitch, EmoteProvider::SevenTv])
            .with_proxy("https://api.test/emotes");
        let requests = loader.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].provider, EmoteProvider::Twitch);
        assert_eq!(requests[0].via, Via::Proxy);
        assert_eq!(
            requests[0].url,
            "https://api.test/emotes/twitch-api/chat/emotes/global"
        );
        assert_eq!(requests[1].via, Via::Direct);
        for request in &requests {
            assert!(loader.answered(request.key, request.via, Ok(fixture(request))));
        }
        assert!(loader.index().get("TwitchUnity").is_some());
        // 7TV's Clap over a Twitch emote of the same code.
        assert_eq!(loader.report()[0].emotes, 6);
        // Without a proxy there is no way to ask.
        let mut loader = SetLoader::for_globals()
            .with_twitch_globals()
            .only(&[EmoteProvider::Twitch]);
        assert!(loader.requests().is_empty());
        assert!(loader.is_done());
        assert_eq!(loader.report()[0].failures, ["no proxy to ask through"]);
    }

    #[test]
    fn a_channel_id_of_another_shape_asks_for_no_channel_set() {
        let mut loader = SetLoader::for_channel("../../me");
        let requests = loader.requests();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|r| r.scope == EmoteScope::Global));
    }
}
