//! When a link sent by e-mail stops working, as its recipient reads it.

use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use mimorya_agent::runtime::french_date;

pub fn expiry_label(expires_at: DateTime<Utc>) -> String {
    format!(
        "{} à {} (heure de Paris)",
        french_date(expires_at, Paris),
        expires_at.with_timezone(&Paris).format("%H:%M")
    )
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
