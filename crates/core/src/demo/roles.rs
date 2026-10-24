//! FICTIONAL demo company "Néo Maintenance" — roles (platform templates plus
//! business roles).

use uuid::Uuid;

use crate::authz::RoleDefinition;
use crate::roles::role_template;

use super::ids::role;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoRole {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub is_system: bool,
}

impl DemoRole {
    pub fn definition(&self) -> RoleDefinition {
        RoleDefinition {
            id: self.id,
            key: self.key.clone(),
            name: self.name.clone(),
            permissions: self.permissions.clone(),
        }
    }
}

fn from_template(id: Uuid, key: &str) -> DemoRole {
    let template = role_template(key).unwrap_or_else(|| panic!("Unknown role template {key}"));
    DemoRole {
        id,
        key: template.key.into(),
        name: template.name.into(),
        description: template.description.into(),
        permissions: template.permissions.iter().map(|p| (*p).into()).collect(),
        is_system: true,
    }
}

fn business(id: Uuid, key: &str, name: &str, description: &str, permissions: &[&str]) -> DemoRole {
    DemoRole {
        id,
        key: key.into(),
        name: name.into(),
        description: description.into(),
        permissions: permissions.iter().map(|p| (*p).into()).collect(),
        is_system: false,
    }
}

pub fn role_definitions() -> Vec<DemoRole> {
    vec![
        from_template(role::MEMBER, "member"),
        from_template(role::TENANT_ADMIN, "tenant_admin"),
        from_template(role::KNOWLEDGE_MANAGER, "knowledge_manager"),
        business(
            role::TECHNICIAN,
            "technician",
            "Technicien",
            "Consulte et complète ses propres interventions.",
            &["intervention:read", "intervention:write"],
        ),
        business(
            role::TEAM_LEAD,
            "team_lead",
            "Responsable d’équipe",
            "Pilote les interventions et le planning de son périmètre.",
            &[
                "intervention:read",
                "planning:read",
                "planning:propose",
                "planning:apply",
                "memory:read",
                "memory:write",
                "document:read",
                "document:write",
                "approval:decide",
            ],
        ),
        business(
            role::DIRECTOR,
            "director",
            "Direction",
            "Vision globale de l’activité, sans gestion technique de l’espace.",
            &[
                "intervention:read",
                "planning:read",
                "planning:propose",
                "memory:write",
                "document:read",
                "document:write",
                "usage:read",
            ],
        ),
        business(
            role::ADMIN_STAFF,
            "admin_staff",
            "Assistant administratif",
            "Consulte les interventions terminées pour la facturation.",
            &["intervention:read"],
        ),
    ]
}
