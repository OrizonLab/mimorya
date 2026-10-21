//! The providers' declarations, keys included, kept in memory for an hour
//! (ADR-065): a sign-in reads them here instead of fetching the discovery
//! document and the key set at its start and at its return. Kept per
//! declared provider, so per company, even when companies share an issuer.
//! Read anew once stale, or when a token's signature fails with the kept
//! keys (the provider rolled them over, `sign_in`). Per server instance,
//! bounded in number: past the bound, the oldest read goes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

/// How long a declaration and its keys serve before being read again.
pub const METADATA_LIFETIME: Duration = Duration::from_secs(60 * 60);

/// Providers kept at most, across companies.
const MAX_PROVIDERS: usize = 256;

pub struct MetadataCache<T> {
    entries: Mutex<HashMap<Uuid, (Instant, T)>>,
}

impl<T> Default for MetadataCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Clone> MetadataCache<T> {
    /// The provider's declaration, if read less than its lifetime ago.
    pub fn get(&self, provider_id: Uuid, now: Instant) -> Option<T> {
        let entries = self.entries.lock().expect("metadata cache");
        entries
            .get(&provider_id)
            .filter(|(read_at, _)| now.saturating_duration_since(*read_at) < METADATA_LIFETIME)
            .map(|(_, metadata)| metadata.clone())
    }

    /// Keeps the provider's declaration, just read.
    pub fn put(&self, provider_id: Uuid, metadata: T, now: Instant) {
        let mut entries = self.entries.lock().expect("metadata cache");
        if !entries.contains_key(&provider_id) && entries.len() >= MAX_PROVIDERS {
            entries.retain(|_, (read_at, _)| {
                now.saturating_duration_since(*read_at) < METADATA_LIFETIME
            });
            let oldest = entries
                .iter()
                .min_by_key(|(_, (read_at, _))| *read_at)
                .map(|(provider_id, _)| *provider_id);
            if let Some(oldest) = oldest.filter(|_| entries.len() >= MAX_PROVIDERS) {
                entries.remove(&oldest);
            }
        }
        entries.insert(provider_id, (now, metadata));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(n: usize) -> Uuid {
        Uuid::from_u128(n as u128 + 1)
    }

    #[test]
    fn a_declaration_serves_for_its_lifetime_then_is_read_again() {
        let cache = MetadataCache::default();
        let read_at = Instant::now();
        assert_eq!(cache.get(provider(0), read_at), None);
        cache.put(provider(0), "v1".to_string(), read_at);
        let almost = read_at + METADATA_LIFETIME - Duration::from_secs(1);
        assert_eq!(cache.get(provider(0), almost).as_deref(), Some("v1"));
        assert_eq!(cache.get(provider(0), read_at + METADATA_LIFETIME), None);
        assert_eq!(cache.get(provider(1), read_at), None);
        // Read again, it serves anew.
        let later = read_at + METADATA_LIFETIME;
        cache.put(provider(0), "v2".to_string(), later);
        assert_eq!(cache.get(provider(0), later).as_deref(), Some("v2"));
    }

    #[test]
    fn past_its_bound_the_oldest_read_goes() {
        let cache = MetadataCache::default();
        let start = Instant::now();
        for n in 0..MAX_PROVIDERS {
            cache.put(provider(n), n, start + Duration::from_secs(n as u64));
        }
        let now = start + Duration::from_secs(MAX_PROVIDERS as u64);
        // Read again: no room taken.
        cache.put(provider(5), 5, now);
        assert_eq!(cache.get(provider(0), now), Some(0));
        let newcomer = provider(MAX_PROVIDERS);
        cache.put(newcomer, 1000, now);
        assert_eq!(cache.get(provider(0), now), None);
        assert_eq!(cache.get(provider(1), now), Some(1));
        assert_eq!(cache.get(newcomer, now), Some(1000));
        assert_eq!(cache.entries.lock().unwrap().len(), MAX_PROVIDERS);
    }

    #[test]
    fn stale_declarations_make_room_first() {
        let cache = MetadataCache::default();
        let start = Instant::now();
        for n in 0..MAX_PROVIDERS {
            cache.put(provider(n), n, start);
        }
        let later = start + METADATA_LIFETIME;
        cache.put(provider(MAX_PROVIDERS), 1000, later);
        assert_eq!(cache.entries.lock().unwrap().len(), 1);
    }
}
