//! What the cache keeps an instance under (ADR-174): whose it is, the
//! company's or one person's on a server each person authorizes, and what
//! it was started with, the connector's version and, for a person's, their
//! authorization's. Each changes on its own: an authorization given anew is
//! a new version, whenever its transaction began.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Whose instance: the company's, or one person's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub instance: Uuid,
    pub person: Option<Uuid>,
}

impl SessionKey {
    /// One person's instance of a server each person authorizes.
    pub fn person(instance: Uuid, person: Uuid) -> Self {
        Self {
            instance,
            person: Some(person),
        }
    }
}

impl From<Uuid> for SessionKey {
    fn from(instance: Uuid) -> Self {
        Self {
            instance,
            person: None,
        }
    }
}

/// What an instance was started with; the later of two is the connector's
/// later version, or the same one with a later authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub connector: DateTime<Utc>,
    /// When the person's authorization was given or last renewed.
    pub authorization: Option<DateTime<Utc>>,
}

impl From<DateTime<Utc>> for Version {
    fn from(connector: DateTime<Utc>) -> Self {
        Self {
            connector,
            authorization: None,
        }
    }
}
