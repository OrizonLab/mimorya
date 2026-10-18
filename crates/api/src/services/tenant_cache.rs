//! Short-lived per-tenant cache. Every invalidation bumps a generation: a
//! value loaded across an invalidation is not stored, since it may predate the
//! change that caused it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

struct State<T> {
    entries: HashMap<Uuid, (Instant, Arc<T>)>,
    generation: u64,
}

pub struct TenantCache<T> {
    ttl: Duration,
    state: Mutex<State<T>>,
}

impl<T> TenantCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            state: Mutex::new(State {
                entries: HashMap::new(),
                generation: 0,
            }),
        }
    }

    /// The fresh value of the tenant, or else the generation to hand back to
    /// `store` once the value is loaded.
    pub fn get(&self, tenant_id: Uuid) -> Result<Arc<T>, u64> {
        let state = self.state.lock().expect("tenant cache");
        match state.entries.get(&tenant_id) {
            Some((loaded_at, value)) if loaded_at.elapsed() < self.ttl => Ok(value.clone()),
            _ => Err(state.generation),
        }
    }

    /// Stores a value loaded since `get` returned `generation`, unless an
    /// invalidation happened meanwhile. Returns whether it was stored.
    pub fn store(&self, tenant_id: Uuid, generation: u64, value: Arc<T>) -> bool {
        let mut state = self.state.lock().expect("tenant cache");
        if state.generation != generation {
            return false;
        }
        state.entries.insert(tenant_id, (Instant::now(), value));
        true
    }

    pub fn invalidate(&self, tenant_id: Uuid) {
        let mut state = self.state.lock().expect("tenant cache");
        state.entries.remove(&tenant_id);
        state.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TENANT: Uuid = Uuid::from_u128(1);

    #[test]
    fn serves_stored_values_until_invalidated() {
        let cache = TenantCache::new(Duration::from_secs(30));
        let generation = cache.get(TENANT).unwrap_err();
        assert!(cache.store(TENANT, generation, Arc::new("v1")));
        assert_eq!(*cache.get(TENANT).unwrap(), "v1");
        cache.invalidate(TENANT);
        assert!(cache.get(TENANT).is_err());
    }

    #[test]
    fn drops_values_loaded_across_an_invalidation() {
        let cache = TenantCache::new(Duration::from_secs(30));
        let generation = cache.get(TENANT).unwrap_err();
        // The change commits and invalidates while the old data is loading.
        cache.invalidate(TENANT);
        assert!(!cache.store(TENANT, generation, Arc::new("stale")));
        assert!(cache.get(TENANT).is_err());
        let generation = cache.get(TENANT).unwrap_err();
        assert!(cache.store(TENANT, generation, Arc::new("fresh")));
        assert_eq!(*cache.get(TENANT).unwrap(), "fresh");
    }

    #[test]
    fn expires_values_after_their_lifetime() {
        let cache = TenantCache::new(Duration::ZERO);
        let generation = cache.get(TENANT).unwrap_err();
        assert!(cache.store(TENANT, generation, Arc::new("v1")));
        assert!(cache.get(TENANT).is_err());
    }
}
