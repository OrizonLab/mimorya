//! The places of a heavy piece of work a process runs a few at once, one
//! at a time for each holder: a company's audit chain verification
//! (ADR-179, ADR-180), a person's export of their own data (ADR-187). No
//! holder takes every place from the others. A place is given back when
//! dropped, its holder's with it.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use mimorya_core::DomainError;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

/// The places of one kind of work in this process, and their holders.
#[derive(Clone)]
pub struct Places {
    places: Arc<Semaphore>,
    holders: Arc<Mutex<HashSet<Uuid>>>,
    /// Said when every place is taken.
    busy: &'static str,
    /// Said when the holder already has one.
    held: &'static str,
}

/// A place, held until dropped.
pub struct Place {
    _place: OwnedSemaphorePermit,
    holders: Arc<Mutex<HashSet<Uuid>>>,
    holder: Uuid,
}

impl Drop for Place {
    fn drop(&mut self) {
        let mut holders = self.holders.lock().unwrap_or_else(PoisonError::into_inner);
        holders.remove(&self.holder);
    }
}

impl Places {
    pub fn new(count: usize, busy: &'static str, held: &'static str) -> Self {
        Self {
            places: Arc::new(Semaphore::new(count)),
            holders: Arc::default(),
            busy,
            held,
        }
    }

    /// A place for `holder`; refused while it holds one, or while every
    /// place is taken.
    pub fn take(&self, holder: Uuid) -> Result<Place, DomainError> {
        let mut holders = self.holders.lock().unwrap_or_else(PoisonError::into_inner);
        if holders.contains(&holder) {
            return Err(DomainError::conflict(self.held));
        }
        let place = Arc::clone(&self.places)
            .try_acquire_owned()
            .map_err(|_| DomainError::unavailable(self.busy))?;
        holders.insert(holder);
        Ok(Place {
            _place: place,
            holders: Arc::clone(&self.holders),
            holder,
        })
    }
}
