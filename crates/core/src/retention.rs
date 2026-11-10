//! How long a company keeps what retention purges (DATA_MODEL §5,
//! ADR-195): its idle conversations, its episodes and the personal memories
//! of its accounts gone, each for the duration it sets within the
//! platform's bounds, the platform's own without its choice. A shortening
//! waits `SHORTENING_DELAY_DAYS` before the purge applies it; a lengthening
//! applies at once. The database holds the same durations, bounds and rule
//! (`retention_platform`, `retention_allowed`, `retention_in_effect`): what
//! it purges never rests on the application alone.

use crate::DomainError;

/// How long a shortening waits before the purge applies it.
pub const SHORTENING_DELAY_DAYS: i32 = 7;

/// What retention purges, by the duration a company sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RetentionKind {
    /// Conversations idle this many months, whole (ADR-186).
    Conversations,
    /// Episodes recorded this many months ago (ADR-185).
    Episodes,
    /// Personal memories of an account disabled this many days ago.
    Departed,
}

/// The unit of a kind's duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionUnit {
    Months,
    Days,
}

impl RetentionKind {
    pub const ALL: [Self; 3] = [Self::Conversations, Self::Episodes, Self::Departed];

    /// How the database names it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Conversations => "conversations",
            Self::Episodes => "episodes",
            Self::Departed => "departed",
        }
    }

    pub fn of(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    pub fn unit(self) -> RetentionUnit {
        match self {
            Self::Conversations | Self::Episodes => RetentionUnit::Months,
            Self::Departed => RetentionUnit::Days,
        }
    }

    /// The platform's duration, without the company's choice.
    pub fn platform(self) -> i32 {
        match self {
            Self::Conversations => 24,
            Self::Episodes => 12,
            Self::Departed => 30,
        }
    }

    /// The shortest a company may set.
    pub fn min(self) -> i32 {
        match self {
            Self::Conversations | Self::Episodes => 1,
            Self::Departed => 7,
        }
    }

    /// The longest a company may set.
    pub fn max(self) -> i32 {
        match self {
            Self::Conversations => 60,
            Self::Episodes => 36,
            Self::Departed => 90,
        }
    }

    /// What it is, as an administrator reads it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Conversations => "Conversations inactives",
            Self::Episodes => "Mémoires épisodiques",
            Self::Departed => "Mémoires personnelles d’un compte désactivé",
        }
    }

    /// `duration`, if a company may set it; otherwise why not, in French.
    pub fn check(self, duration: i32) -> Result<i32, DomainError> {
        if (self.min()..=self.max()).contains(&duration) {
            return Ok(duration);
        }
        let unit = match self.unit() {
            RetentionUnit::Months => "mois",
            RetentionUnit::Days => "jours",
        };
        Err(DomainError::validation(format!(
            "{} : de {} à {} {unit}",
            self.label(),
            self.min(),
            self.max()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_s_platform_duration_is_within_its_bounds() {
        for kind in RetentionKind::ALL {
            assert_eq!(kind.check(kind.platform()), Ok(kind.platform()));
            assert_eq!(RetentionKind::of(kind.key()), Some(kind));
        }
        assert_eq!(RetentionKind::of("sessions"), None);
    }

    #[test]
    fn a_duration_out_of_bounds_is_refused_with_them() {
        let kind = RetentionKind::Departed;
        assert_eq!(kind.check(7), Ok(7));
        assert_eq!(kind.check(90), Ok(90));
        let refused = kind.check(6).unwrap_err();
        assert_eq!(
            refused.message,
            "Mémoires personnelles d’un compte désactivé : de 7 à 90 jours"
        );
        assert!(kind.check(91).is_err());
        let conversations = RetentionKind::Conversations;
        assert!(conversations.check(0).is_err());
        assert!(conversations.check(61).is_err());
        assert_eq!(
            conversations.check(0).unwrap_err().message,
            "Conversations inactives : de 1 à 60 mois"
        );
    }
}
