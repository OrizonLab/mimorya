//! Access tokens kept between a run's calls (ADR-117), per company and
//! person, in this process only: a token serves until shortly before it
//! expires, and only while the link is the one it was renewed from (its
//! connector and version, `refreshed_at`, read at each call). A token the
//! provider refused is forgotten. Bounded in number: past the bound, the
//! expired go, then all.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Renewed this long before it expires: a call started with it ends first.
pub const EXPIRY_MARGIN: Duration = Duration::from_secs(120);
/// How long a token serves when the provider does not say.
const DEFAULT_LIFETIME: Duration = Duration::from_secs(300);
const MAX_KEPT: usize = 10_000;

struct Kept {
    connector_id: Uuid,
    version: DateTime<Utc>,
    token: String,
    until: Instant,
}

/// The link a token was renewed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkVersion {
    pub connector_id: Uuid,
    pub refreshed_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct AccessTokens {
    kept: Mutex<HashMap<(Uuid, Uuid), Kept>>,
}

impl AccessTokens {
    /// The person's token, if renewed from this link and still valid.
    pub fn get(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        link: LinkVersion,
        now: Instant,
    ) -> Option<String> {
        let kept = self.kept.lock().expect("access tokens");
        kept.get(&(tenant_id, user_id))
            .filter(|kept| kept.connector_id == link.connector_id)
            .filter(|kept| kept.version == link.refreshed_at && now < kept.until)
            .map(|kept| kept.token.clone())
    }

    /// Keeps a token just renewed from this link, valid for `lifetime` if
    /// the provider said.
    pub fn put(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        link: LinkVersion,
        token: String,
        lifetime: Option<Duration>,
        now: Instant,
    ) {
        let lifetime = lifetime.unwrap_or(DEFAULT_LIFETIME);
        let until = now + lifetime.saturating_sub(EXPIRY_MARGIN);
        let mut kept = self.kept.lock().expect("access tokens");
        if kept.len() >= MAX_KEPT {
            kept.retain(|_, kept| now < kept.until);
            if kept.len() >= MAX_KEPT {
                kept.clear();
            }
        }
        kept.insert(
            (tenant_id, user_id),
            Kept {
                connector_id: link.connector_id,
                version: link.refreshed_at,
                token,
                until,
            },
        );
    }

    /// The provider refused the person's token: the next call renews it.
    pub fn forget(&self, tenant_id: Uuid, user_id: Uuid) {
        self.kept
            .lock()
            .expect("access tokens")
            .remove(&(tenant_id, user_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_serves_its_link_until_shortly_before_it_expires() {
        let tokens = AccessTokens::default();
        let (tenant, user) = (Uuid::new_v4(), Uuid::new_v4());
        let link = LinkVersion {
            connector_id: Uuid::new_v4(),
            refreshed_at: Utc::now(),
        };
        let now = Instant::now();
        let hour = Duration::from_secs(3600);
        tokens.put(tenant, user, link, "a".into(), Some(hour), now);
        assert_eq!(tokens.get(tenant, user, link, now).as_deref(), Some("a"));
        let late = now + hour - EXPIRY_MARGIN;
        assert_eq!(tokens.get(tenant, user, link, late), None);
        // Another person, another version of the link, another connector.
        assert_eq!(tokens.get(tenant, Uuid::new_v4(), link, now), None);
        let renewed = LinkVersion {
            refreshed_at: link.refreshed_at + chrono::Duration::seconds(1),
            ..link
        };
        assert_eq!(tokens.get(tenant, user, renewed, now), None);
        let other = LinkVersion {
            connector_id: Uuid::new_v4(),
            ..link
        };
        assert_eq!(tokens.get(tenant, user, other, now), None);
        tokens.forget(tenant, user);
        assert_eq!(tokens.get(tenant, user, link, now), None);
    }

    #[test]
    fn a_token_whose_lifetime_is_unknown_serves_briefly() {
        let tokens = AccessTokens::default();
        let (tenant, user) = (Uuid::new_v4(), Uuid::new_v4());
        let link = LinkVersion {
            connector_id: Uuid::new_v4(),
            refreshed_at: Utc::now(),
        };
        let now = Instant::now();
        tokens.put(tenant, user, link, "a".into(), None, now);
        let later = now + DEFAULT_LIFETIME - EXPIRY_MARGIN;
        assert_eq!(tokens.get(tenant, user, link, later), None);
    }
}
