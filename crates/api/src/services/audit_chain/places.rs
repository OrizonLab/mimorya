//! The places of the verifications of their audit chain administrators ask
//! for (ADR-179, ADR-180): a process runs a few at once, each reading a
//! whole chain, and one at a time for a company, so that a company never
//! takes every place from the others. A place is given back when dropped,
//! its company's with it.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use mimorya_core::DomainError;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

/// Verifications one process runs at once.
pub const CHECKS_AT_ONCE: usize = 2;

const BUSY: &str = "D’autres vérifications du journal d’audit sont en cours : réessayez dans un \
     instant.";
const VERIFYING: &str = "Une vérification du journal d’audit de votre entreprise est déjà en \
     cours : attendez son résultat.";

/// The places of the verifications this process runs, and their companies.
#[derive(Clone)]
pub struct ChainChecks {
    places: Arc<Semaphore>,
    companies: Arc<Mutex<HashSet<Uuid>>>,
}

impl Default for ChainChecks {
    fn default() -> Self {
        Self {
            places: Arc::new(Semaphore::new(CHECKS_AT_ONCE)),
            companies: Arc::default(),
        }
    }
}

/// A verification's place, held until dropped.
pub struct CheckPlace {
    _place: OwnedSemaphorePermit,
    companies: Arc<Mutex<HashSet<Uuid>>>,
    tenant_id: Uuid,
}

impl Drop for CheckPlace {
    fn drop(&mut self) {
        let mut companies = self
            .companies
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        companies.remove(&self.tenant_id);
    }
}

impl ChainChecks {
    /// A place for a verification of the company `tenant_id`; refused while
    /// the company has one, or while this process runs as many as it may.
    pub fn take(&self, tenant_id: Uuid) -> Result<CheckPlace, DomainError> {
        let mut companies = self
            .companies
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if companies.contains(&tenant_id) {
            return Err(DomainError::conflict(VERIFYING));
        }
        let place = Arc::clone(&self.places)
            .try_acquire_owned()
            .map_err(|_| DomainError::unavailable(BUSY))?;
        companies.insert(tenant_id);
        Ok(CheckPlace {
            _place: place,
            companies: Arc::clone(&self.companies),
            tenant_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use mimorya_core::ErrorCode;

    use super::*;

    fn company(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn a_process_runs_a_few_verifications_one_a_company() {
        let checks = ChainChecks::default();
        let first = checks.take(company(1)).unwrap();
        // The same company waits for its verification, whatever is free.
        let again = checks.take(company(1)).err().map(|refused| refused.code);
        assert_eq!(again, Some(ErrorCode::Conflict));
        let others: Vec<_> = (2..=CHECKS_AT_ONCE as u128)
            .map(|n| checks.take(company(n)).unwrap())
            .collect();
        let full = checks.take(company(99)).err().map(|refused| refused.code);
        assert_eq!(full, Some(ErrorCode::Unavailable));
        // A place given back is its company's again, and anyone's.
        drop(first);
        assert!(checks.take(company(1)).is_ok());
        drop(others);
        let places: Vec<_> = (10..10 + CHECKS_AT_ONCE as u128)
            .map(|n| checks.take(company(n)))
            .collect();
        assert!(places.iter().all(Result::is_ok));
    }
}
