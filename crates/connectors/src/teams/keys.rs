//! The keys the Bot Framework signs its activities with (ADR-138): read
//! from its OpenID configuration (its `jwks_uri`), kept a day, read again
//! sooner when an activity names a key not kept, but at most every five
//! minutes, so that a key Microsoft adds is known within minutes and a
//! stream of forged activities does not make the platform ask Microsoft
//! again and again. Microsoft unreachable, the keys read last are kept.

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
/// The most keys kept: Microsoft publishes a handful.
const MAX_KEYS: usize = 64;

/// The keys read, and when.
#[derive(Debug, Clone)]
pub(super) struct KeyCache {
    keys: Vec<BotKey>,
    read_at: Instant,
}

impl TeamsApi {
    /// The keys the Bot Framework signs with, for a token signed with
    /// `wanted`.
    pub async fn keys(&self, wanted: Option<&str>) -> Result<Vec<BotKey>, TeamsFailure> {
        let cached = self.cached_keys();
        if let Some(cache) = &cached {
            let age = cache.read_at.elapsed();
            let known = wanted.is_none_or(|kid| cache.keys.iter().any(|key| key.kid == kid));
            if age < READ_AGAIN_AFTER || (age < KEPT_FOR && known) {
                return Ok(cache.keys.clone());
            }
        }
        match self.read_keys().await {
            Ok(keys) => {
                self.keep_keys(KeyCache {
                    keys: keys.clone(),
                    read_at: Instant::now(),
                });
                Ok(keys)
            }
            Err(failure) => match cached {
                Some(cache) => {
                    let reason = failure.message();
                    tracing::warn!(%reason, "Bot Framework keys not read again");
                    Ok(cache.keys)
                }
                None => Err(failure),
            },
        }
    }

    /// The keys, as the Bot Framework's configuration says where.
    async fn read_keys(&self) -> Result<Vec<BotKey>, TeamsFailure> {
        let configuration = self.get_json(self.openid().clone()).await?;
        let address = (configuration.get("jwks_uri").and_then(Value::as_str))
            .and_then(|uri| Url::parse(uri).ok())
            .filter(|uri| matches!(uri.scheme(), "https" | "http"))
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
