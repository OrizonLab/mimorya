//! Platform role templates created for every new tenant. Tenants add their own
//! business roles (technician, team lead…) on top. Note that the technical
//! administrator manages the workspace but receives no business-data access.

#[derive(Debug, Clone, Copy)]
pub struct RoleTemplate {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub permissions: &'static [&'static str],
}

pub const PLATFORM_ROLE_TEMPLATES: &[RoleTemplate] = &[
    RoleTemplate {
        key: "member",
        name: "Collaborateur",
        description: "Accès de base : converser avec l’agent, consulter l’organisation, la mémoire et les documents.",
        permissions: &[
            "agent:use",
            "org:read",
            "user:read",
            "memory:read",
            "document:read",
            "mission:create",
            "mission:read",
            "skill:read",
            "skill:propose",
            "connector:use",
        ],
    },
    RoleTemplate {
        key: "tenant_admin",
        name: "Administrateur technique",
        description: "Gère l’espace entreprise (comptes, rôles, connecteurs, compétences). Ne donne pas accès aux données métier.",
        permissions: &[
            "org:read",
            "org:manage",
            "user:read",
            "user:manage",
            "role:manage",
            "policy:manage",
            "connector:manage",
            "skill:manage",
            "settings:manage",
            "memory:manage",
            "audit:read",
            "usage:read",
        ],
    },
    RoleTemplate {
        key: "knowledge_manager",
        name: "Gestionnaire des connaissances",
        description: "Alimente et valide la mémoire partagée et les documents de son périmètre.",
        permissions: &[
            "memory:read",
            "memory:write",
            "document:read",
            "document:write",
            "skill:read",
            "skill:propose",
        ],
    },
];

pub fn role_template(key: &str) -> Option<&'static RoleTemplate> {
    PLATFORM_ROLE_TEMPLATES.iter().find(|t| t.key == key)
}
