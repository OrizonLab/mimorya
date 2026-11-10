//! What the platform's operator does from the command line (`mimorya-admin`,
//! ADR-033), outside any company's administration: create a company with its
//! first administrator, give an account that never chose its password a new
//! invitation link, suspend and reactivate a company (ADR-191), withdraw
//! what a suspended one declared at other services (ADR-193). Each change
//! runs in the company's own transaction (row-level security applies) with
//! its audit entries, by the `system` actor. The links are returned to the
//! operator only: never stored in clear, logged nor audited. Deleting a
//! suspended company (ADR-192) alone runs as the database's owner.

mod company;
mod deletion;
mod invitation;
mod release;
mod suspension;

pub use company::{CreatedCompany, NewCompany, create_company};
pub use deletion::{CompanyDeletion, CompanyRows, delete_company};
pub use invitation::{OperatorInvitation, reinvite};
pub use release::{BotReleased, Released, ServerReleased, release_company, release_company_with};
pub use suspension::{CompanyStatusChange, change_company_status};

use mimorya_db::repositories::audit::AuditEntry;
use uuid::Uuid;

fn system_entry(tenant_id: Uuid, action: &str, resource_type: &str, id: Uuid) -> AuditEntry {
    AuditEntry::new(tenant_id, "system", action, "success").resource(resource_type, id)
}
