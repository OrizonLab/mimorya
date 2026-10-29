//! The permissions a Slack application's token has, as Slack says them with
//! each answer (`x-oauth-scopes`, ADR-132): read when its token is given and
//! whenever an administrator checks it, compared with those the application
//! asks for (`BOT_SCOPES`). One missing, and the gateway cannot do what it
//! must (ADR-131: without `im:read` it answers nobody); one more than asked
//! for, and the platform keeps a token stronger than it needs.

use super::BOT_SCOPES;

/// The most permissions read: Slack's whole list is far shorter.
pub const MAX_SCOPES: usize = 200;
/// The longest a permission's name is read.
const MAX_SCOPE_CHARS: usize = 80;

/// The permissions Slack says a token has, from its `x-oauth-scopes`
/// header (names separated by commas), each once; `None` when it does not
/// read as Slack writes them.
pub fn granted_scopes(header: &str) -> Option<Vec<String>> {
    let mut scopes: Vec<String> = Vec::new();
    for scope in header.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !is_scope(scope) || scopes.len() == MAX_SCOPES {
            return None;
        }
        if !scopes.iter().any(|known| known == scope) {
            scopes.push(scope.to_owned());
        }
    }
    Some(scopes)
}

/// Whether `scope` is written as Slack names a permission (`chat:write`,
/// `users:read.email`).
fn is_scope(scope: &str) -> bool {
    scope.len() <= MAX_SCOPE_CHARS
        && scope.starts_with(|c: char| c.is_ascii_lowercase())
        && scope
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b":._-".contains(&b))
}

/// What the application asks for that `granted` lacks, in the order it
/// asks for them.
pub fn missing_scopes(granted: &[String]) -> Vec<&'static str> {
    (BOT_SCOPES.iter().copied())
        .filter(|asked| !granted.iter().any(|scope| scope == asked))
        .collect()
}

/// What `granted` has beyond what the application asks for, sorted.
pub fn extra_scopes(granted: &[String]) -> Vec<String> {
    let mut extra: Vec<String> = (granted.iter())
        .filter(|scope| !BOT_SCOPES.contains(&scope.as_str()))
        .cloned()
        .collect();
    extra.sort();
    extra
}

/// Why a token lacking `missing` is not taken, in French.
pub fn missing_scopes_refusal(missing: &[&str]) -> String {
    let (them, it) = if missing.len() == 1 {
        ("la permission", "la")
    } else {
        ("les permissions", "les")
    };
    format!(
        "Il manque à l’application Slack {them} {} : ajoutez-{it} dans « OAuth & Permissions », \
         réinstallez l’application, puis donnez son jeton du bot",
        missing.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(scopes: &[&str]) -> Vec<String> {
        scopes.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_permissions_are_read_as_slack_writes_them() {
        assert_eq!(
            granted_scopes("chat:write,im:history, users:read.email ,chat:write"),
            Some(owned(&["chat:write", "im:history", "users:read.email"]))
        );
        assert_eq!(granted_scopes(""), Some(Vec::new()));
        assert_eq!(granted_scopes(" , "), Some(Vec::new()));
        for odd in [
            "chat:write,<script>",
            "Chat:write",
            "chat write",
            "chat:write;im:read",
            ":write",
            &"a".repeat(81),
        ] {
            assert_eq!(granted_scopes(odd), None, "{odd}");
        }
        let many = (0..=MAX_SCOPES)
            .map(|n| format!("scope{n}:read"))
            .collect::<Vec<_>>();
        assert_eq!(granted_scopes(&many.join(",")), None);
        assert_eq!(
            granted_scopes(&many[..MAX_SCOPES].join(",")).map(|s| s.len()),
            Some(MAX_SCOPES)
        );
    }

    #[test]
    fn what_is_missing_and_what_is_more_are_told_apart() {
        let all = owned(&BOT_SCOPES);
        assert!(missing_scopes(&all).is_empty());
        assert!(extra_scopes(&all).is_empty());
        let without = owned(&["users:read.email", "chat:write", "im:history", "users:read"]);
        assert_eq!(missing_scopes(&without), ["im:read"]);
        let more = owned(&["files:read", "chat:write", "channels:history"]);
        assert_eq!(
            missing_scopes(&more),
            ["im:history", "im:read", "users:read", "users:read.email"]
        );
        assert_eq!(extra_scopes(&more), ["channels:history", "files:read"]);
    }

    #[test]
    fn a_refusal_names_what_is_missing() {
        assert_eq!(
            missing_scopes_refusal(&["im:read"]),
            "Il manque à l’application Slack la permission im:read : ajoutez-la dans « OAuth & \
             Permissions », réinstallez l’application, puis donnez son jeton du bot"
        );
        assert!(
            missing_scopes_refusal(&["im:read", "users:read"]).starts_with(
                "Il manque à l’application Slack les permissions im:read, users:read : ajoutez-les"
            )
        );
    }
}
