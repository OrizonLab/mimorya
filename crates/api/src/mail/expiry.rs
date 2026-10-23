//! A moment as an e-mail's recipient reads it, in Paris time: when a link
//! stops working, when an absence starts and ends.

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use mimorya_agent::runtime::french_date;

/// « mardi 14 juillet 2026 à 23:05 », in Paris time, which the sentence says.
pub fn moment_label(at: DateTime<Utc>) -> String {
    format!(
        "{} à {}",
        french_date(at, Paris),
        at.with_timezone(&Paris).format("%H:%M")
    )
}

pub fn expiry_label(expires_at: DateTime<Utc>) -> String {
    format!("{} (heure de Paris)", moment_label(expires_at))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn reads_in_paris_time() {
        let summer = Utc.with_ymd_and_hms(2026, 7, 14, 21, 5, 0).unwrap();
        assert_eq!(
            expiry_label(summer),
            "mardi 14 juillet 2026 à 23:05 (heure de Paris)"
        );
    }
}
