//! The providers' declarations, keys included, kept in memory for an hour
//! (ADR-065): a sign-in reads them here instead of fetching the discovery
//! document and the key set at its start and at its return. Read anew once
//! stale, or when a token is signed by a key the kept set lacks (the
//! provider rolled its keys over, `sign_in`); an administrator's check of
//! an issuer always reads it anew. Failures are not kept. Per server
//! instance, bounded in number: past the bound, the oldest read goes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a declaration and its keys serve before being read again.
pub const METADATA_LIFETIME: Duration = Duration::from_secs(60 * 60);

/// Issuers kept at most: one per provider declared, across companies.
const MAX_ISSUERS: usize = 256;

pub struct MetadataCache<T> {
    entries: Mutex<HashMap<String, (Instant, T)>>,
}

impl<T> Default for MetadataCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Clone> MetadataCache<T> {
    /// The issuer's declaration, if read less than its lifetime ago.
    pub fn get(&self, issuer: &str, now: Instant) -> Option<T> {
        let entries = self.entries.lock().expect("metadata cache");
        entries
            .get(issuer)
            .filter(|(read_at, _)| now.saturating_duration_since(*read_at) < METADATA_LIFETIME)
            .map(|(_, metadata)| metadata.clone())
    }

    /// Keeps the issuer's declaration, just read.
    pub fn put(&self, issuer: &str, metadata: T, now: Instant) {
        let mut entries = self.entries.lock().expect("metadata cache");
        if !entries.contains_key(issuer) && entries.len() >= MAX_ISSUERS {
            entries.retain(|_, (read_at, _)| {
                now.saturating_duration_since(*read_at) < METADATA_LIFETIME
            });
            let oldest = entries
                .iter()
                .min_by_key(|(_, (read_at, _))| *read_at)
                .map(|(issuer, _)| issuer.clone());
            if let Some(oldest) = oldest.filter(|_| entries.len() >= MAX_ISSUERS) {
                entries.remove(&oldest);
            }
        }
        entries.insert(issuer.to_string(), (now, metadata));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_serves_for_its_lifetime_then_is_read_again() {
        let cache = MetadataCache::default();
        let read_at = Instant::now();
        assert_eq!(cache.get("https://idp.example", read_at), None);
        cache.put("https://idp.example", "v1".to_string(), read_at);
        let almost = read_at + METADATA_LIFETIME - Duration::from_secs(1);
        assert_eq!(
            cache.get("https://idp.example", almost).as_deref(),
            Some("v1")
        );
        assert_eq!(
            cache.get("https://idp.example", read_at + METADATA_LIFETIME),
            None
        );
        assert_eq!(cache.get("https://autre.example", read_at), None);
        // Read again, it serves anew.
        let later = read_at + METADATA_LIFETIME;
        cache.put("https://idp.example", "v2".to_string(), later);
        assert_eq!(
            cache.get("https://idp.example", later).as_deref(),
            Some("v2")
        );
    }

    #[test]
    fn past_its_bound_the_oldest_read_goes() {
        let cache = MetadataCache::default();
        let start = Instant::now();
        let issuer = |n: usize| format!("https://idp-{n}.example");
        for n in 0..MAX_ISSUERS {
            cache.put(&issuer(n), n, start + Duration::from_secs(n as u64));
        }
        let now = start + Duration::from_secs(MAX_ISSUERS as u64);
        // Read again: no room taken.
        cache.put(&issuer(5), 5, now);
        assert_eq!(cache.get(&issuer(0), now), Some(0));
        cache.put("https://nouveau.example", 1000, now);
        assert_eq!(cache.get(&issuer(0), now), None);
        assert_eq!(cache.get(&issuer(1), now), Some(1));
        assert_eq!(cache.get("https://nouveau.example", now), Some(1000));
        assert_eq!(cache.entries.lock().unwrap().len(), MAX_ISSUERS);
    }

    #[test]
    fn stale_declarations_make_room_first() {
        let cache = MetadataCache::default();
        let start = Instant::now();
        for n in 0..MAX_ISSUERS {
            cache.put(&format!("https://idp-{n}.example"), n, start);
        }
        let later = start + METADATA_LIFETIME;
        cache.put("https://nouveau.example", 1000, later);
        assert_eq!(cache.entries.lock().unwrap().len(), 1);
    }
}
