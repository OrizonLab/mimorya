//! The places of the verifications of their audit chain administrators ask
//! for (ADR-179, ADR-180): a process runs a few at once, each reading a
//! whole chain, and one at a time for a company, so that a company never
//! takes every place from the others. A place is given back when dropped,
//! its company's with it.

use mimorya_core::DomainError;
use uuid::Uuid;

use crate::services::places::{Place, Places};

/// Verifications one process runs at once.
pub const CHECKS_AT_ONCE: usize = 2;

const BUSY: &str = "D’autres vérifications du journal d’audit sont en cours : réessayez dans un \
     instant.";
const VERIFYING: &str = "Une vérification du journal d’audit de votre entreprise est déjà en \
     cours : attendez son résultat.";

/// A verification's place, held until dropped.
pub type CheckPlace = Place;

/// The places of the verifications this process runs, and their companies.
#[derive(Clone)]
pub struct ChainChecks(Places);

impl Default for ChainChecks {
    fn default() -> Self {
        Self(Places::new(CHECKS_AT_ONCE, BUSY, VERIFYING))
    }
}

impl ChainChecks {
    /// A place for a verification of the company `tenant_id`; refused while
    /// the company has one, or while this process runs as many as it may.
    pub fn take(&self, tenant_id: Uuid) -> Result<CheckPlace, DomainError> {
        self.0.take(tenant_id)
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
