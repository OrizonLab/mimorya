//! Demo staff directory: technician names and scope-based selection.

use mimorya_core::authz::{DataScope, scope_includes};
use mimorya_core::demo::{self, role};
use uuid::Uuid;

pub fn technician_name(id: Uuid) -> String {
    demo::users()
        .into_iter()
        .find(|u| u.id == id)
        .map_or_else(|| "Inconnu".to_string(), |u| u.display_name)
}

/// Technicians whose schedule falls within a data scope, in directory order.
pub fn technicians_in_scope(scope: &DataScope) -> Vec<Uuid> {
    demo::users()
        .into_iter()
        .filter(|u| u.assignments.iter().any(|a| a.role_id == role::TECHNICIAN))
        .filter(|u| scope_includes(scope, Some(u.primary_unit_id), Some(u.id)))
        .map(|u| u.id)
        .collect()
}
