//! What the platform's operator does from the command line (`mimorya-admin`,
//! ADR-033), outside any company's administration: create a company with its
//! first administrator, and give an account that never chose its password a
//! new invitation link. Each change runs in the company's own transaction
//! (row-level security applies) with its audit entries, by the `system`
//! actor. The links are returned to the operator only: never stored in
//! clear, logged nor audited.

mod company;
mod invitation;

pub use company::{CreatedCompany, NewCompany, create_company};
pub use invitation::{OperatorInvitation, reinvite};

use mimorya_db::repositories::audit::AuditEntry;
use uuid::Uuid;

fn system_entry(tenant_id: Uuid, action: &str, resource_type: &str, id: Uuid) -> AuditEntry {
    AuditEntry::new(tenant_id, "system", action, "success").resource(resource_type, id)
}
