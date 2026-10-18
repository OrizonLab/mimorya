//! Data classification levels, ordered from least to most sensitive.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    Public,
    /// Default clearance of a user.
    #[default]
    Internal,
    Confidential,
    Restricted,
}

impl Sensitivity {
    pub const ALL: [Sensitivity; 4] = [
        Self::Public,
        Self::Internal,
        Self::Confidential,
        Self::Restricted,
    ];

    pub fn rank(self) -> u8 {
        self as u8
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }
}

impl fmt::Display for Sensitivity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Sensitivity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|level| level.as_str() == value)
            .ok_or_else(|| format!("unknown sensitivity level `{value}`"))
    }
}
