//! The archive of the WAL (ADR-206): on only when the stack was given the
//! backups' recipients (`MIMORYA_BACKUP_RECIPIENTS`), as the database's
//! entrypoint turns it on, and how its last segments went.

use chrono::{DateTime, Utc};
use mimorya_db::Database;
use mimorya_db::repositories::installation::{WalArchive, wal_archive};

use super::{Finding, State};
use crate::mail::moment_label;

pub(super) const TOPIC: &str = "Sauvegardes";

pub(super) async fn state(db: &Database) -> Finding {
    match wal_archive(db.pool()).await {
        Ok(archive) => said(&archive),
        Err(error) => Finding::new(TOPIC, State::Failed, format!("illisible ({error})")),
    }
}

pub(super) fn said(archive: &WalArchive) -> Finding {
    if !matches!(archive.mode.as_str(), "on" | "always") {
        return Finding::new(
            TOPIC,
            State::ToComplete,
            "les WAL ne sont pas archivés : aucun destinataire des sauvegardes \
             (MIMORYA_BACKUP_RECIPIENTS) ; seules les sauvegardes lancées par \
             infra/backup/backup.sh protègent la base (docs/RECOVERY_PLAN.md)",
        );
    }
    let after = |failed: DateTime<Utc>| archive.last_archived_at.is_none_or(|at| failed > at);
    if let Some(failed) = archive.last_failed_at.filter(|failed| after(*failed)) {
        return Finding::new(
            TOPIC,
            State::Failed,
            format!(
                "le dernier archivage d’un WAL a échoué, le {} (heure de Paris) : voir les \
                 journaux de la base (docker compose … logs db)",
                moment_label(failed)
            ),
        );
    }
    let last = match archive.last_archived_at {
        Some(at) => format!(", le dernier le {} (heure de Paris)", moment_label(at)),
        None => ", aucun encore : le premier part quand un segment est rempli".to_string(),
    };
    Finding::new(
        TOPIC,
        State::Ready,
        format!(
            "WAL archivés, chiffrés pour les destinataires des sauvegardes : {} archivés{last} ; \
             l’état des sauvegardes de base : infra/backup/archive-status.sh",
            archive.archived
        ),
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn archive(mode: &str, failed_after: bool) -> WalArchive {
        let at = |hour| Utc.with_ymd_and_hms(2026, 11, 22, hour, 0, 0).single();
        WalArchive {
            mode: mode.into(),
            archived: 42,
            failed: u8::from(failed_after).into(),
            last_archived_at: at(10),
            last_failed_at: failed_after.then(|| at(11)).flatten(),
        }
    }

    #[test]
    fn an_archive_is_said_as_it_goes() {
        let off = said(&archive("off", false));
        assert_eq!(off.state, State::ToComplete);
        assert!(off.said.contains("MIMORYA_BACKUP_RECIPIENTS"));
        let on = said(&archive("on", false));
        assert_eq!(on.state, State::Ready);
        assert!(
            on.said
                .contains("42 archivés, le dernier le dimanche 22 novembre"),
            "{}",
            on.said
        );
        let failing = said(&archive("on", true));
        assert_eq!(failing.state, State::Failed);
        assert!(failing.said.contains("a échoué"), "{}", failing.said);
        // A failure the archive got over since is no longer one.
        let mut recovered = archive("on", true);
        recovered.last_archived_at = Utc.with_ymd_and_hms(2026, 11, 22, 12, 0, 0).single();
        assert_eq!(said(&recovered).state, State::Ready);
    }
}
