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

/// Where the platform checks a permission. It bounds what an access policy can
/// restrict (ADR-022): a deny on a permission never checked would have no
/// effect, and a condition on the resource only applies where each resource is
/// checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    /// Declared but not checked anywhere yet.
    NotYet,
    /// Checked on the person holding it, and on the acting agent: routes,
    /// tools offered and run, list scopes (`data_scope`).
    Holder,
    /// Also checked on each resource (`check`), resource attributes included.
    PerResource,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PermissionInfo {
    pub permission: &'static str,
    pub label: &'static str,
    /// Permissions an agent may never exercise on behalf of a user.
    pub delegable: bool,
    pub enforcement: Enforcement,
}

const fn info(
    permission: &'static str,
    label: &'static str,
    delegable: bool,
    enforcement: Enforcement,
) -> PermissionInfo {
    PermissionInfo {
        permission,
        label,
        delegable,
        enforcement,
    }
}

use Enforcement::{Holder, NotYet};

/// Platform permission catalog. Connectors register their own domain
/// permissions (e.g. `intervention:read`) through the connector registry.
pub const PLATFORM_PERMISSIONS: &[PermissionInfo] = &[
    info("agent:use", "Converser avec l'agent", true, Holder),
    info("org:read", "Consulter l'organisation", true, Holder),
    info("org:manage", "Gérer l'organisation", false, Holder),
    info("user:read", "Consulter l’annuaire", true, Holder),
    info("user:manage", "Gérer les comptes", false, Holder),
    info(
        "role:manage",
        "Gérer les rôles et habilitations",
        false,
        Holder,
    ),
    info(
        "policy:manage",
        "Gérer les politiques d’accès",
        false,
        Holder,
    ),
    info("memory:read", "Lire la mémoire partagée", true, Holder),
    info(
        "memory:write",
        "Écrire dans la mémoire partagée",
        true,
        Holder,
    ),
    info("memory:manage", "Administrer la mémoire", false, Holder),
    info("mission:create", "Confier des missions", true, NotYet),
    info("mission:read", "Suivre les missions", true, NotYet),
    // Approvals are decided by the person who asked for the run.
    info(
        "approval:decide",
        "Valider les actions proposées",
        false,
        NotYet,
    ),
    info("skill:read", "Consulter les compétences", true, Holder),
    info(
        "skill:manage",
        "Gérer et activer les compétences",
        false,
        Holder,
    ),
    info("connector:use", "Utiliser les connecteurs", true, Holder),
    info(
        "connector:manage",
        "Configurer les connecteurs",
        false,
        Holder,
    ),
    info("audit:read", "Consulter le journal d’audit", false, Holder),
    info(
        "settings:manage",
        "Paramétrer l’espace entreprise",
        false,
        Holder,
    ),
    info("usage:read", "Consulter la consommation", true, Holder),
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
