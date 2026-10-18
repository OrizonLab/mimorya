//! Rules for the roles a company defines itself: which permissions they may
//! grant and how they are keyed. Platform roles (`is_system`) are fixed; the
//! API also applies the administration rules (company-wide scope for
//! administration permissions, no change to a role one holds).

/// At most this many permissions in one role.
pub const MAX_ROLE_PERMISSIONS: usize = 100;

/// The permissions of a custom role, trimmed, deduplicated and sorted, or why
/// they are refused (in French). Only permissions of the catalog are accepted,
/// written out: no wildcard, so that a role never grows with the catalog.
pub fn normalize_role_permissions(
    requested: &[String],
    catalog: &[&str],
) -> Result<Vec<String>, String> {
    let mut permissions: Vec<String> = requested.iter().map(|p| p.trim().to_string()).collect();
    permissions.sort();
    permissions.dedup();
    if permissions.is_empty() {
        return Err("Un rôle doit accorder au moins une permission".into());
    }
    if permissions.len() > MAX_ROLE_PERMISSIONS {
        return Err(format!(
            "Un rôle accorde au plus {MAX_ROLE_PERMISSIONS} permissions"
        ));
    }
    if let Some(unknown) = permissions.iter().find(|p| !catalog.contains(&p.as_str())) {
        return Err(format!("Permission inconnue : {unknown}"));
    }
    Ok(permissions)
}

/// Stable key of a custom role, derived from its name when it is created
/// ("Chef de chantier" → `custom_chef_de_chantier`); `None` when the name has
/// no letter or digit. The prefix keeps it apart from the platform roles.
pub fn custom_role_key(name: &str) -> Option<String> {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        match fold(c) {
            Some(letters) => slug.push_str(letters),
            None if c.is_ascii_alphanumeric() => slug.push(c),
            None if !slug.is_empty() && !slug.ends_with('_') => slug.push('_'),
            None => {}
        }
    }
    let slug = slug.trim_end_matches('_');
    (!slug.is_empty()).then(|| format!("custom_{slug}"))
}

/// Accented French letters, without their accent.
fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'â' | 'ä' => "a",
        'ç' => "c",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'î' | 'ï' => "i",
        'ô' | 'ö' => "o",
        'ù' | 'û' | 'ü' => "u",
        'ÿ' => "y",
        'œ' => "oe",
        'æ' => "ae",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &[&str] = &["intervention:read", "planning:apply", "user:manage"];

    fn perms(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn accepts_catalog_permissions_only() {
        assert_eq!(
            normalize_role_permissions(
                &perms(&[" planning:apply", "intervention:read", "planning:apply"]),
                CATALOG
            ),
            Ok(perms(&["intervention:read", "planning:apply"]))
        );
        assert!(normalize_role_permissions(&[], CATALOG).is_err());
        assert_eq!(
            normalize_role_permissions(&perms(&["intervention:*"]), CATALOG),
            Err("Permission inconnue : intervention:*".into())
        );
        assert!(normalize_role_permissions(&perms(&["*"]), CATALOG).is_err());
    }

    #[test]
    fn derives_keys_from_names() {
        assert_eq!(
            custom_role_key("Chef de chantier").as_deref(),
            Some("custom_chef_de_chantier")
        );
        assert_eq!(
            custom_role_key("  Équipe d’astreinte (nuit) ").as_deref(),
            Some("custom_equipe_d_astreinte_nuit")
        );
        assert_eq!(custom_role_key("Cœur 2").as_deref(), Some("custom_coeur_2"));
        assert_eq!(custom_role_key("— !"), None);
    }
}
