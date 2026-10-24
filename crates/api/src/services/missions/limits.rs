//! A mission's own limits (ADR-089): so many stages, so much cost, beyond
//! which it ends unfinished. The platform's, unless the company set its own
//! within what the platform allows (ADR-093); kept on each mission as when
//! it was entrusted.

use mimorya_agent::runtime::MISSION_STAGE_BUDGET;
use mimorya_core::DomainError;
use mimorya_db::repositories::mission_settings::{MissionSettingsRow, mission_settings};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

/// A mission's own limits, kept as when it was entrusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionLimits {
    pub max_stages: u32,
    pub max_cost_micros: i64,
}

/// Six stages, 12 USD: four times a stage's cost, with stages to spare for
/// those that cost less. A company without limits of its own has these.
pub const MISSION_LIMITS: MissionLimits = MissionLimits {
    max_stages: 6,
    max_cost_micros: 12_000_000,
};

/// What a company may set (ADR-093): from one stage to twenty, from 1 USD
/// to what twenty stages may cost (60 USD): more would never be reached.
/// The database holds the same bounds.
pub const MAX_STAGES_CEILING: u32 = 20;
pub const MIN_COST_MICROS: i64 = 1_000_000;
pub const MAX_COST_CEILING_MICROS: i64 =
    MAX_STAGES_CEILING as i64 * MISSION_STAGE_BUDGET.max_cost_micros;

/// Why limits are refused: the bounds, as they stand.
fn bad_limits() -> String {
    let dollars = |micros: i64| micros / 1_000_000;
    format!(
        "Une mission compte de 1 à {MAX_STAGES_CEILING} étapes au plus, et coûte de {} à {} \
         USD au plus.",
        dollars(MIN_COST_MICROS),
        dollars(MAX_COST_CEILING_MICROS)
    )
}

impl MissionLimits {
    /// The limits a company sets, if within what the platform allows.
    pub fn checked(max_stages: u32, max_cost_micros: i64) -> Result<Self, DomainError> {
        let stages = (1..=MAX_STAGES_CEILING).contains(&max_stages);
        let cost = (MIN_COST_MICROS..=MAX_COST_CEILING_MICROS).contains(&max_cost_micros);
        if !(stages && cost) {
            return Err(DomainError::validation(bad_limits()));
        }
        Ok(Self {
            max_stages,
            max_cost_micros,
        })
    }
}

/// The company's own limits, in the company `conn` reads in, if it set some:
/// without them, the platform's apply (`MISSION_LIMITS`).
pub async fn company_limits(conn: &mut PgConnection) -> anyhow::Result<Option<MissionLimits>> {
    let Some(MissionSettingsRow {
        max_stages,
        max_cost_micros,
    }) = mission_settings(conn).await?
    else {
        return Ok(None);
    };
    Ok(Some(MissionLimits {
        max_stages: u32::try_from(max_stages)?,
        max_cost_micros,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_company_sets_its_limits_within_the_platform_s() {
        let checked = |stages, cost| MissionLimits::checked(stages, cost).map_err(|e| e.message);
        assert_eq!(
            checked(1, MIN_COST_MICROS),
            Ok(MissionLimits {
                max_stages: 1,
                max_cost_micros: MIN_COST_MICROS
            })
        );
        assert!(checked(MAX_STAGES_CEILING, MAX_COST_CEILING_MICROS).is_ok());
        for (stages, cost) in [
            (0, MIN_COST_MICROS),
            (MAX_STAGES_CEILING + 1, MIN_COST_MICROS),
            (1, MIN_COST_MICROS - 1),
            (1, MAX_COST_CEILING_MICROS + 1),
            (1, -1),
        ] {
            assert_eq!(checked(stages, cost), Err(bad_limits()), "{stages} {cost}");
        }
        let platform = MISSION_LIMITS;
        assert!(checked(platform.max_stages, platform.max_cost_micros).is_ok());
        assert_eq!(MAX_COST_CEILING_MICROS, 60_000_000);
        assert_eq!(
            bad_limits(),
            "Une mission compte de 1 à 20 étapes au plus, et coûte de 1 à 60 USD au plus."
        );
    }
}
