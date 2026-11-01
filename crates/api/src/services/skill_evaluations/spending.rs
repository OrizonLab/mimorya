//! What a company's skill evaluations may cost (ADR-154): over the last
//! thirty days, the platform's limit unless the company set its own within
//! the platform's bounds. Each case played adds its cost to the company's
//! spending as it is recorded, whether or not its evaluation still runs:
//! it was spent. An evaluation is refused once the limit is reached, and
//! one under way ends between two cases when it is: it goes over by one
//! case at most.

use mimorya_core::DomainError;
use mimorya_db::repositories::evaluation_settings::evaluation_limit;
use mimorya_db::repositories::evaluation_spending::{add_evaluation_spending, evaluation_spending};
use sqlx::PgConnection;
use uuid::Uuid;

/// The days the limit counts, today included.
pub const SPENDING_WINDOW_DAYS: i32 = 30;
/// The days of spending kept: the window, and some to spare.
const KEPT_DAYS: i32 = 40;

/// 50 USD over thirty days: five evaluations of ten cases, each case played
/// for two versions at 0.50 USD at most. A company without its own limit
/// has this one.
pub const PLATFORM_EVALUATION_LIMIT_MICROS: i64 = 50_000_000;
/// What a company may set: from 1 USD, a case or two…
pub const MIN_EVALUATION_LIMIT_MICROS: i64 = 1_000_000;
/// …to 1000 USD, a hundred evaluations of ten cases. The database holds the
/// same bounds.
pub const MAX_EVALUATION_LIMIT_MICROS: i64 = 1_000_000_000;

/// What the company's evaluations cost over the window, against its limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spending {
    pub spent_micros: i64,
    pub limit_micros: i64,
    /// Whether the limit is the company's own, not the platform's.
    pub own: bool,
}

impl Spending {
    pub fn reached(&self) -> bool {
        self.spent_micros >= self.limit_micros
    }

    /// Why no evaluation may run any more, as its administrators read it.
    pub fn refusal(&self) -> String {
        format!(
            "Les évaluations de l’entreprise ont coûté {} sur les trente derniers jours, pour une \
             limite de {} : ceux qui gèrent ses réglages peuvent la relever.",
            usd(self.spent_micros),
            usd(self.limit_micros)
        )
    }
}

/// The company's spending and limit, in the company `conn` reads in.
pub async fn spending(conn: &mut PgConnection) -> anyhow::Result<Spending> {
    let own = evaluation_limit(&mut *conn).await?;
    let spent_micros = evaluation_spending(conn, SPENDING_WINDOW_DAYS).await?;
    Ok(Spending {
        spent_micros,
        limit_micros: own.unwrap_or(PLATFORM_EVALUATION_LIMIT_MICROS),
        own: own.is_some(),
    })
}

/// Counts what a case cost in the company's spending.
pub async fn count_spent(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    cost_micros: i64,
) -> anyhow::Result<()> {
    add_evaluation_spending(conn, tenant_id, cost_micros, KEPT_DAYS).await?;
    Ok(())
}

/// The limit a company sets, if within the platform's bounds.
pub fn checked_limit(max_cost_micros: i64) -> Result<i64, DomainError> {
    if (MIN_EVALUATION_LIMIT_MICROS..=MAX_EVALUATION_LIMIT_MICROS).contains(&max_cost_micros) {
        Ok(max_cost_micros)
    } else {
        Err(DomainError::validation(bad_limit()))
    }
}

/// Why a limit is refused: the bounds, as they stand.
fn bad_limit() -> String {
    format!(
        "La limite des évaluations sur trente jours va de {} à {}.",
        usd(MIN_EVALUATION_LIMIT_MICROS),
        usd(MAX_EVALUATION_LIMIT_MICROS)
    )
}

/// An amount in micro-dollars, as the French read it: `12,34 USD`.
fn usd(micros: i64) -> String {
    let cents = micros.max(0) / 10_000;
    format!("{},{:02} USD", cents / 100, cents % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_company_sets_its_limit_within_the_platform_s() {
        let checked = |micros| checked_limit(micros).map_err(|e| e.message);
        assert_eq!(checked(1_000_000), Ok(1_000_000));
        assert_eq!(checked(1_000_000_000), Ok(1_000_000_000));
        assert_eq!(checked(PLATFORM_EVALUATION_LIMIT_MICROS), Ok(50_000_000));
        for refused in [999_999, 1_000_000_001, 0, -1] {
            assert_eq!(checked(refused), Err(bad_limit()), "{refused}");
        }
        assert_eq!(
            bad_limit(),
            "La limite des évaluations sur trente jours va de 1,00 USD à 1000,00 USD."
        );
    }

    #[test]
    fn the_limit_is_reached_once_spent() {
        let at = |spent_micros| Spending {
            spent_micros,
            limit_micros: 2_500_000,
            own: true,
        };
        assert!(!at(2_499_999).reached());
        assert!(at(2_500_000).reached());
        assert!(at(3_120_000).reached());
        assert_eq!(
            at(3_125_678).refusal(),
            "Les évaluations de l’entreprise ont coûté 3,12 USD sur les trente derniers jours, \
             pour une limite de 2,50 USD : ceux qui gèrent ses réglages peuvent la relever."
        );
    }
}
