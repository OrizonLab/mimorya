//! The database as the API reaches it (its application role), the
//! migrations it holds against those this version carries, and the stack's
//! processes listening on it.

use mimorya_db::migrate::MIGRATOR;
use mimorya_db::repositories::installation::{
    AppliedMigration, Listeners, applied_migrations, listeners,
};
use mimorya_db::{Database, PoolOptions};

use super::{Finding, State};
use crate::config::AppConfig;

pub(super) const TOPIC: &str = "Base de données";
const MIGRATIONS: &str = "Migrations";
pub(super) const WORKERS: &str = "Workers";

pub(super) async fn connect(config: &AppConfig) -> Result<Database, String> {
    let options = PoolOptions {
        max_connections: 1,
        application_name: "mimorya-admin",
    };
    let db = Database::connect(&config.database_app_url, options)
        .await
        .map_err(|error| format!("injoignable ({error})"))?;
    if let Err(error) = sqlx::query("select 1").execute(db.pool()).await {
        return Err(format!("ne répond pas ({error})"));
    }
    Ok(db)
}

pub(super) fn reached() -> Finding {
    Finding::new(TOPIC, State::Ready, "joignable par le rôle applicatif")
}

pub(super) async fn migrations(db: &Database) -> Finding {
    match applied_migrations(db.pool()).await {
        Ok(applied) => compare(&carried(), &applied),
        Err(error) => Finding::new(
            MIGRATIONS,
            State::Failed,
            format!(
                "journal illisible ({error}) : les migrations de cette version sont-elles \
                 appliquées ? (service migrate, mimorya-admin migrate)"
            ),
        ),
    }
}

/// The migrations this version carries: version and checksum.
fn carried() -> Vec<(i64, Vec<u8>)> {
    MIGRATOR
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| (migration.version, migration.checksum.to_vec()))
        .collect()
}

/// The journal against what this version carries.
pub(super) fn compare(carried: &[(i64, Vec<u8>)], applied: &[AppliedMigration]) -> Finding {
    let failed = |said: String| Finding::new(MIGRATIONS, State::Failed, said);
    if let Some(broken) = applied.iter().find(|m| !m.success) {
        return failed(format!(
            "la migration {:04} a échoué : à reprendre avant tout (journaux du service migrate)",
            broken.version
        ));
    }
    let carried_version = |version: i64| carried.iter().find(|(v, _)| *v == version);
    if let Some(changed) = applied
        .iter()
        .find(|m| carried_version(m.version).is_some_and(|(_, sum)| *sum != m.checksum))
    {
        return failed(format!(
            "la migration {:04} a changé depuis qu’elle a été appliquée : cette version n’est \
             pas celle de la base",
            changed.version
        ));
    }
    if let Some(unknown) = applied
        .iter()
        .find(|m| carried_version(m.version).is_none())
    {
        return failed(format!(
            "la base porte la migration {:04}, que cette version ne connaît pas : l’image est \
             plus ancienne que la base (MIMORYA_VERSION)",
            unknown.version
        ));
    }
    let pending: Vec<i64> = carried
        .iter()
        .map(|(version, _)| *version)
        .filter(|version| !applied.iter().any(|m| m.version == *version))
        .collect();
    if let Some(first) = pending.first() {
        return failed(format!(
            "{} à appliquer, depuis la {first:04} : docker compose … up -d les applique \
             (service migrate)",
            plural(pending.len() as i64, "migration", "migrations")
        ));
    }
    let last = applied.last().map_or(0, |m| m.version);
    Finding::new(
        MIGRATIONS,
        State::Ready,
        format!(
            "à jour : {}, la dernière {last:04}",
            plural(
                applied.len() as i64,
                "migration appliquée",
                "migrations appliquées"
            )
        ),
    )
}

pub(super) async fn workers(db: &Database) -> Finding {
    match listeners(db.pool()).await {
        Ok(listening) => listening_said(listening),
        Err(error) => Finding::new(WORKERS, State::Failed, format!("illisible ({error})")),
    }
}

/// The workers and API processes listening, as their connections show.
pub(super) fn listening_said(listening: Listeners) -> Finding {
    match (listening.queue, listening.events) {
        (0, _) => Finding::new(
            WORKERS,
            State::Failed,
            "aucun worker à l’écoute de la file des runs : les demandes restent en attente \
             (docker compose … ps worker)",
        ),
        (queue, 0) => Finding::new(
            WORKERS,
            State::Failed,
            format!(
                "{} à l’écoute, mais aucun processus api : personne ne reçoit les demandes",
                plural(queue, "worker", "workers")
            ),
        ),
        (queue, events) => Finding::new(
            WORKERS,
            State::Ready,
            format!(
                "{} à l’écoute de la file des runs, {} relayant leurs événements",
                plural(queue, "worker", "workers"),
                plural(events, "processus api", "processus api")
            ),
        ),
    }
}

/// « 1 worker », « 2 workers ».
pub(super) fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count < 2 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(version: i64, success: bool, checksum: u8) -> AppliedMigration {
        AppliedMigration {
            version,
            success,
            checksum: vec![checksum],
        }
    }

    #[test]
    fn the_journal_is_compared_with_what_this_version_carries() {
        let carried = [(1, vec![1]), (2, vec![2]), (3, vec![3])];
        let current = [
            applied(1, true, 1),
            applied(2, true, 2),
            applied(3, true, 3),
        ];
        let ready = compare(&carried, &current);
        assert_eq!(ready.state, State::Ready);
        assert_eq!(
            ready.said,
            "à jour : 3 migrations appliquées, la dernière 0003"
        );
        let behind = compare(&carried, &current[..1]);
        assert_eq!(behind.state, State::Failed);
        assert!(
            behind
                .said
                .starts_with("2 migrations à appliquer, depuis la 0002"),
            "{}",
            behind.said
        );
        let ahead = compare(&carried[..2], &current);
        assert!(
            ahead
                .said
                .contains("migration 0003, que cette version ne connaît pas")
        );
        let changed = compare(&carried, &[applied(1, true, 9)]);
        assert!(changed.said.contains("0001 a changé"), "{}", changed.said);
        let broken = compare(&carried, &[applied(1, true, 1), applied(2, false, 2)]);
        assert!(
            broken.said.starts_with("la migration 0002 a échoué"),
            "{}",
            broken.said
        );
    }

    #[test]
    fn the_listening_processes_say_who_serves() {
        let said = |queue, events| listening_said(Listeners { queue, events });
        assert_eq!(said(2, 1).state, State::Ready);
        assert_eq!(
            said(2, 1).said,
            "2 workers à l’écoute de la file des runs, 1 processus api relayant leurs événements"
        );
        assert!(said(0, 1).said.starts_with("aucun worker"));
        assert_eq!(said(1, 0).state, State::Failed);
        assert_eq!(plural(1, "worker", "workers"), "1 worker");
    }
}
