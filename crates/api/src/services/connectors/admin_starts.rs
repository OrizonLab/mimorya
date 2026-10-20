//! The connector starts an administration asks for (testing a saved
//! connector, trying a server before saving it) are bounded per company
//! (ADR-055): each holds a slot while its start lasts, released when it
//! ends, even after its client has left, so that a company's administrators
//! cannot keep the platform busy with starts. Runs' starts are not counted:
//! one start is shared by the runs that need it, then kept (ADR-049).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use mimorya_core::DomainError;
use uuid::Uuid;

/// Starts an administration may have under way at once, per company.
pub const MAX_ADMIN_STARTS: usize = 2;

/// Why another start is refused while those are under way.
pub const TOO_MANY_ADMIN_STARTS: &str = "Deux démarrages de connecteur sont déjà en cours dans \
     votre entreprise : réessayez quand l’un d’eux aura fini.";

type Running = Arc<Mutex<HashMap<Uuid, usize>>>;

/// The starts under way, by company.
#[derive(Default)]
pub struct AdminStarts {
    running: Running,
}

/// A slot held for one start; dropping it frees the slot.
#[must_use = "dropping the slot frees it at once"]
pub struct AdminStart {
    tenant_id: Uuid,
    running: Running,
}

impl AdminStarts {
    /// A slot for a start in `tenant_id`, refused while the company already
    /// has `MAX_ADMIN_STARTS` under way.
    pub fn claim(&self, tenant_id: Uuid) -> Result<AdminStart, DomainError> {
        let mut running = lock(&self.running);
        let count = running.entry(tenant_id).or_insert(0);
        if *count >= MAX_ADMIN_STARTS {
            return Err(DomainError::conflict(TOO_MANY_ADMIN_STARTS));
        }
        *count += 1;
        Ok(AdminStart {
            tenant_id,
            running: self.running.clone(),
        })
    }
}

impl Drop for AdminStart {
    fn drop(&mut self) {
        let mut running = lock(&self.running);
        if let Some(count) = running.get_mut(&self.tenant_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                running.remove(&self.tenant_id);
            }
        }
    }
}

/// The counts stay right if a holder panicked: each change is one step.
fn lock(running: &Running) -> MutexGuard<'_, HashMap<Uuid, usize>> {
    running
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_the_starts_of_each_company_and_frees_a_slot_when_a_start_ends() {
        let starts = AdminStarts::default();
        let (company, other) = (Uuid::new_v4(), Uuid::new_v4());
        let first = starts.claim(company).unwrap();
        let _second = starts.claim(company).unwrap();
        let refused = starts.claim(company).err().unwrap();
        assert_eq!(refused.message, TOO_MANY_ADMIN_STARTS);
        // Another company is not held back.
        let _elsewhere = starts.claim(other).unwrap();
        drop(first);
        let _third = starts.claim(company).unwrap();
        assert!(starts.claim(company).is_err());
    }
}
