//! When a message its messaging service did not take is tried again: a
//! run's answer (ADR-127, ADR-135, ADR-140) or a manager's notice (ADR-130,
//! ADR-137), in Slack, in Telegram and in Teams. Each service says what it refused,
//! never tried again, and what it could not take now, tried again here.

use chrono::{DateTime, Duration, Utc};
use mimorya_db::repositories::gateway_replies::ReplyRecord;

/// The longest wait before a message is tried again, whatever the service
/// says.
const MAX_WAIT: Duration = Duration::hours(1);
/// The attempts a message is given while its service cannot be reached.
pub const MAX_ATTEMPTS: i32 = 4;

/// When a message its service did not take, at its `attempt`-th try, is
/// tried again: 30 s, 2 min, 8 min, or when the service says (`told`),
/// within an hour; never past `MAX_ATTEMPTS`.
pub fn later(attempt: i32, told: Option<std::time::Duration>, now: DateTime<Utc>) -> ReplyRecord {
    if attempt >= MAX_ATTEMPTS {
        return ReplyRecord::Done("failed");
    }
    let wait = match told {
        Some(after) => Duration::from_std(after).unwrap_or(MAX_WAIT),
        None => Duration::seconds(30 * 4_i64.pow(attempt.saturating_sub(1) as u32)),
    };
    ReplyRecord::Retry(now + wait.min(MAX_WAIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_not_taken_is_tried_again_a_few_times_within_an_hour() {
        let now = Utc::now();
        assert_eq!(
            later(1, None, now),
            ReplyRecord::Retry(now + Duration::seconds(30))
        );
        assert_eq!(
            later(3, None, now),
            ReplyRecord::Retry(now + Duration::minutes(8))
        );
        assert_eq!(later(MAX_ATTEMPTS, None, now), ReplyRecord::Done("failed"));
        let told = Some(std::time::Duration::from_secs(12));
        assert_eq!(
            later(1, told, now),
            ReplyRecord::Retry(now + Duration::seconds(12))
        );
        // Whatever the service says, within an hour.
        let far = Some(std::time::Duration::from_secs(100_000_000_000_000));
        assert_eq!(
            later(1, far, now),
            ReplyRecord::Retry(now + Duration::hours(1))
        );
    }
}
