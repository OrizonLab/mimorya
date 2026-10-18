//! Permissions are `domain:action` strings. Roles bundle permissions; role
//! assignments give them a scope (see `engine.rs`). A pattern may use `*` as
//! the action (`memory:*`) or as the whole permission (`*`) — patterns are
//! only accepted in role definitions and policies, never in checks.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

static PERMISSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_.-]*:[a-z][a-z0-9_.-]*$").expect("valid regex"));
static PATTERN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\*|[a-z][a-z0-9_.-]*:(\*|[a-z][a-z0-9_.-]*))$").expect("valid regex")
});

pub fn is_permission(value: &str) -> bool {
    PERMISSION_RE.is_match(value)
}

pub fn is_permission_pattern(value: &str) -> bool {
    PATTERN_RE.is_match(value)
}

pub fn matches_permission(pattern: &str, permission: &str) -> bool {
    if pattern == "*" || pattern == permission {
        return true;
    }
    match (pattern.split_once(':'), permission.split_once(':')) {
        (Some((pattern_domain, "*")), Some((domain, _))) => pattern_domain == domain,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PermissionInfo {
    pub permission: &'static str,
    pub label: &'static str,
    /// Permissions an agent may never exercise on behalf of a user.
    pub delegable: bool,
}

const fn info(permission: &'static str, label: &'static str, delegable: bool) -> PermissionInfo {
    PermissionInfo {
        permission,
        label,
        delegable,
    }
}

/// Platform permission catalog. Connectors register their own domain
/// permissions (e.g. `intervention:read`) through the connector registry.
pub const PLATFORM_PERMISSIONS: &[PermissionInfo] = &[
    info("agent:use", "Converser avec l'agent", true),
    info("org:read", "Consulter l'organisation", true),
    info("org:manage", "Gérer l'organisation", false),
    info("user:read", "Consulter l’annuaire", true),
    info("user:manage", "Gérer les comptes", false),
    info("role:manage", "Gérer les rôles et habilitations", false),
    info("policy:manage", "Gérer les politiques d’accès", false),
    info("memory:read", "Lire la mémoire partagée", true),
    info("memory:write", "Écrire dans la mémoire partagée", true),
    info("memory:manage", "Administrer la mémoire", false),
    info("mission:create", "Confier des missions", true),
    info("mission:read", "Suivre les missions", true),
    info("approval:decide", "Valider les actions proposées", false),
    info("skill:read", "Consulter les compétences", true),
    info("skill:manage", "Gérer et activer les compétences", false),
    info("connector:use", "Utiliser les connecteurs", true),
    info("connector:manage", "Configurer les connecteurs", false),
    info("audit:read", "Consulter le journal d’audit", false),
    info("settings:manage", "Paramétrer l’espace entreprise", false),
    info("usage:read", "Consulter la consommation", true),
];

/// Hard ceiling for agents: whatever the user's rights, an agent run can never
/// exercise these. This is what prevents a prompt injection or a self-improved
/// skill from granting privileges or altering security rules.
pub static NON_DELEGABLE_PERMISSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    PLATFORM_PERMISSIONS
        .iter()
        .filter(|p| !p.delegable)
        .map(|p| p.permission)
        .collect()
});

pub fn is_non_delegable(permission: &str) -> bool {
    NON_DELEGABLE_PERMISSIONS.contains(permission)
}
