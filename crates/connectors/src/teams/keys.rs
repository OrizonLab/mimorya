//! The keys the Bot Framework signs its activities with (ADR-138): read
//! from its OpenID configuration (its `jwks_uri`, of the configuration's
//! own scheme), kept a day, read again sooner when an activity names a key
//! not kept, but at most every five minutes, so that a key Microsoft adds
//! is known within minutes and a stream of forged activities does not make
//! the platform ask Microsoft again and again. One read at a time: whoever
//! finds one under way waits for it, then takes what it kept, so that the
//! activities arriving together (as on a restart) are all proven. A failed
//! read is tried again thirty seconds after it ended at the soonest, the
//! keys read last kept meanwhile; none kept, an activity is then proven by
//! none.

use std::time::{Duration, Instant};

use mimorya_core::gateways::teams::BotKey;
use serde_json::Value;
use url::Url;

use super::TeamsFailure;
use super::api::TeamsApi;

/// How long the keys read are kept.
const KEPT_FOR: Duration = Duration::from_secs(24 * 3600);
/// How soon they are read again for a key not kept.
const READ_AGAIN_AFTER: Duration = Duration::from_secs(5 * 60);
/// How soon a read is tried again after the last one, failed or not.
const TRY_AGAIN_AFTER: Duration = Duration::from_secs(30);
/// The most keys kept: Microsoft publishes a handful.
const MAX_KEYS: usize = 64;

/// The keys read, when, and when they were last tried.
#[derive(Debug, Clone)]
pub(super) struct KeyCache {
    keys: Vec<BotKey>,
    read_at: Option<Instant>,
    tried_at: Instant,
}

impl KeyCache {
    /// Whether the keys are to be read again for a token signed with
    /// `wanted`.
    fn due(&self, wanted: Option<&str>) -> bool {
        if self.tried_at.elapsed() < TRY_AGAIN_AFTER {
            return false;
        }
        let Some(read_at) = self.read_at else {
            return true;
        };
        let known = wanted.is_none_or(|kid| self.keys.iter().any(|key| key.kid == kid));
        read_at.elapsed() >= KEPT_FOR || (!known && read_at.elapsed() >= READ_AGAIN_AFTER)
    }
}

impl TeamsApi {
    /// The keys the Bot Framework signs with, for a token signed with
    /// `wanted`: those kept, or read again when due. An error only when
    /// the read tried now fails with none kept.
    pub async fn keys(&self, wanted: Option<&str>) -> Result<Vec<BotKey>, TeamsFailure> {
        let due = |cache: &Option<KeyCache>| cache.as_ref().is_none_or(|c| c.due(wanted));
        let kept = |cache: Option<KeyCache>| cache.map(|c| c.keys).unwrap_or_default();
        let cached = self.cached_keys();
        if !due(&cached) {
            return Ok(kept(cached));
        }
        // One read at a time: the others wait for it, then take what it
        // kept (it is no longer due).
        let _reading = self.reading().lock().await;
        let cached = self.cached_keys();
        if !due(&cached) {
            return Ok(kept(cached));
        }
        let read = self.read_keys().await;
        let now = Instant::now();
        match read {
            Ok(keys) => {
                self.keep_keys(KeyCache {
                    keys: keys.clone(),
                    read_at: Some(now),
                    tried_at: now,
                });
                Ok(keys)
            }
            Err(failure) => {
                let read_at = cached.as_ref().and_then(|c| c.read_at);
                let keys = kept(cached);
                self.keep_keys(KeyCache {
                    keys: keys.clone(),
                    read_at,
                    tried_at: now,
                });
                if read_at.is_none() {
                    return Err(failure);
                }
                let reason = failure.message();
                tracing::warn!(%reason, "Bot Framework keys not read again");
                Ok(keys)
            }
        }
    }

    /// The keys, as the Bot Framework's configuration says where.
    async fn read_keys(&self) -> Result<Vec<BotKey>, TeamsFailure> {
        let configuration = self.get_json(self.openid().clone()).await?;
        let scheme = self.openid().scheme().to_owned();
        let address = (configuration.get("jwks_uri").and_then(Value::as_str))
            .and_then(|uri| Url::parse(uri).ok())
            .filter(|uri| uri.scheme() == scheme)
            .ok_or(TeamsFailure::Unreadable)?;
        let set = self.get_json(address).await?;
        let keys: Vec<BotKey> = (set.get("keys").and_then(Value::as_array))
            .ok_or(TeamsFailure::Unreadable)?
            .iter()
            .filter_map(|key| serde_json::from_value::<BotKey>(key.clone()).ok())
            .filter(|key| key.kty == "RSA")
            .take(MAX_KEYS)
            .collect();
        if keys.is_empty() {
            return Err(TeamsFailure::Unreadable);
        }
        Ok(keys)
    }
}
