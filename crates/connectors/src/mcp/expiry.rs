//! A session started with a token that expires, the access token of an MCP
//! server's OAuth authorization (ADR-171): not started if the token would
//! not last, retired before it expires, its end dated then (ADR-165), for
//! the platform to start it again with the token it reads anew.

use std::time::Duration;

use tokio::time::Instant;

/// A session is retired this long before its token expires: a call under
/// way is not cut by the server's refusal.
pub const RETIRE_BEFORE: Duration = Duration::from_secs(60);

/// A token that expires sooner does not start a session: it would be
/// retired at once. Checked before the start and again once started, a
/// session then lives two minutes at least from its start, more than a
/// session judged a failed start for having closed so soon (ADR-163).
pub const MIN_TOKEN_LIFE: Duration = Duration::from_secs(180);

/// An authorization whose token lasts less when issued is refused: a
/// session could hardly start with it before it expires.
pub const MIN_GRANTED_LIFE: Duration = Duration::from_secs(300);

/// What a server whose authorization's token cannot be sent as kept says.
pub const OAUTH_UNUSABLE: &str = "Le jeton de l’autorisation OAuth n’a pas un format accepté : \
     un administrateur doit autoriser de nouveau ce connecteur";

/// What a server whose authorization's token expired, or is about to, says.
pub const OAUTH_EXPIRED: &str = "Jeton de l’autorisation OAuth expiré : un administrateur doit \
     autoriser de nouveau ce connecteur";

/// When a session is retired, if its token expires.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Expiry {
    retire_at: Option<Instant>,
}

impl Expiry {
    /// The expiry of a session started `now` with a token that lasts until
    /// `until`, if it says; refused if it would not last.
    pub fn of(until: Option<Instant>, now: Instant) -> Result<Self, &'static str> {
        let Some(until) = until else {
            return Ok(Self::default());
        };
        if until < now + MIN_TOKEN_LIFE {
            return Err(OAUTH_EXPIRED);
        }
        Ok(Self {
            retire_at: Some(until - RETIRE_BEFORE),
        })
    }

    /// When the session was retired, once it is.
    pub fn passed(&self, now: Instant) -> Option<Instant> {
        self.retire_at.filter(|at| *at <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_is_retired_before_its_token_expires() {
        let now = Instant::now();
        let lasting = Expiry::of(None, now).expect("started");
        assert_eq!(
            lasting.passed(now + Duration::from_secs(86_400 * 365)),
            None
        );
        let until = now + Duration::from_secs(3600);
        let expiring = Expiry::of(Some(until), now).expect("started");
        let retired = until - RETIRE_BEFORE;
        assert_eq!(expiring.passed(retired - Duration::from_secs(1)), None);
        assert_eq!(expiring.passed(retired), Some(retired));
        assert_eq!(expiring.passed(until), Some(retired));
    }

    #[test]
    fn a_token_that_would_not_last_starts_nothing() {
        let now = Instant::now();
        for left in [0, 60, 179] {
            let until = now + Duration::from_secs(left);
            assert_eq!(Expiry::of(Some(until), now), Err(OAUTH_EXPIRED));
        }
        let lasting = now + MIN_TOKEN_LIFE;
        assert!(Expiry::of(Some(lasting), now).is_ok());
        // Expired already: an instant before now.
        let past = now.checked_sub(Duration::from_secs(5)).unwrap_or(now);
        assert_eq!(Expiry::of(Some(past), now), Err(OAUTH_EXPIRED));
    }
}
