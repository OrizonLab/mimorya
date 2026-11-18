//! How a company's model is supplied (ADR-208), loaded with its models: the
//! formula the operator set, without which the platform's model does not
//! serve it, and the markup of what its calls on the operator's key are
//! re-invoiced; and what each call is charged, by the models it was made
//! with: whoever takes them for a run, a review or a test calls and records
//! by the same, whatever changes meanwhile.

use mimorya_agent::llm::ModelRole;
use mimorya_agent::runtime::UsageRecord;
use mimorya_core::model_supply::{SupplyFormula, billed_micros};
use mimorya_db::TenantTx;
use mimorya_db::repositories::model_supply::model_supply;
use mimorya_db::repositories::usage::UsageCharge;
use uuid::Uuid;

use super::{Models, Resolution, Review};

/// The formula of a company and its markup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Supply {
    pub formula: SupplyFormula,
    pub markup_bps: u32,
}

/// The company's supply as the operator set it: its own key without a row.
pub(super) async fn company_supply(tx: &mut TenantTx, tenant_id: Uuid) -> anyhow::Result<Supply> {
    let Some(row) = model_supply(tx, tenant_id).await? else {
        return Ok(Supply::default());
    };
    let formula = SupplyFormula::parse(&row.formula)
        .ok_or_else(|| anyhow::anyhow!("unknown supply formula {}", row.formula))?;
    Ok(Supply {
        formula,
        // Kept between 0 and 10,000 by the table.
        markup_bps: u32::try_from(row.markup_bps).unwrap_or(0),
    })
}

impl Models {
    /// Whether the company's runs go to the operator's key.
    pub fn runs_on_operator_key(&self) -> bool {
        matches!(self.runs, Resolution::Platform(Some(_)))
    }

    /// What a call of `role` that cost `cost_micros`, recorded now, is
    /// charged: on the company's own key, nothing; on the operator's, its
    /// formula and the amount re-invoiced. A review is on the key of the
    /// model the company chose apart, else on its runs' (ADR-108); every
    /// other role on its runs'.
    pub fn charge(&self, role: ModelRole, cost_micros: i64) -> UsageCharge {
        let on_operator_key = match (role, &self.review) {
            (ModelRole::Review, Review::Company(_)) => false,
            _ => self.runs_on_operator_key(),
        };
        if !on_operator_key {
            return UsageCharge::CLIENT_KEY;
        }
        UsageCharge {
            formula: self.supply.formula,
            billed_micros: Some(billed_micros(cost_micros, self.supply.markup_bps)),
        }
    }

    /// What `record` is charged: a model call by its role, or as the runs'
    /// calls when their own model made it (a review no model chosen apart
    /// gave, ADR-108); any other use, nothing.
    pub fn usage_charge(&self, record: &UsageRecord) -> UsageCharge {
        if record.kind != "llm" {
            return UsageCharge::CLIENT_KEY;
        }
        let role = match record.by_runs_model {
            true => ModelRole::Main,
            false => record.role,
        };
        self.charge(role, record.cost_micros)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mimorya_agent::llm::LlmProvider;
    use mimorya_agent::testing::ScriptedProvider;

    use super::*;

    fn provider() -> Arc<dyn LlmProvider> {
        Arc::new(ScriptedProvider::new(Vec::new()))
    }

    fn models(runs: Resolution, review: Review, formula: SupplyFormula) -> Models {
        Models {
            runs,
            review,
            escalation: None,
            supply: Supply {
                formula,
                markup_bps: 1_500,
            },
        }
    }

    #[test]
    fn a_call_on_the_operator_s_key_carries_its_formula_and_its_re_invoiced_amount() {
        for formula in [SupplyFormula::Capped, SupplyFormula::Prepaid] {
            let supplied = models(
                Resolution::Platform(Some(provider())),
                Review::Runs,
                formula,
            );
            for role in ModelRole::ALL {
                assert_eq!(
                    supplied.charge(role, 200_000),
                    UsageCharge {
                        formula,
                        billed_micros: Some(230_000),
                    },
                    "{role:?}"
                );
            }
            // A review by a model the company chose apart is on its key.
            let apart = models(
                Resolution::Platform(Some(provider())),
                Review::Company(provider()),
                formula,
            );
            assert_eq!(
                apart.charge(ModelRole::Review, 200_000),
                UsageCharge::CLIENT_KEY
            );
            assert_eq!(apart.charge(ModelRole::Main, 200_000).formula, formula);
            // Unless the runs' own model gave it, that one failing.
            let review = |by_runs_model: bool| UsageRecord {
                kind: "llm",
                role: ModelRole::Review,
                by_runs_model,
                provider: "anthropic".into(),
                model: "claude-haiku-5-5".into(),
                input_tokens: 10,
                output_tokens: 10,
                cost_micros: 200_000,
            };
            assert_eq!(apart.usage_charge(&review(false)), UsageCharge::CLIENT_KEY);
            assert_eq!(
                apart.usage_charge(&review(true)),
                apart.charge(ModelRole::Main, 200_000)
            );
        }
    }

    #[test]
    fn a_call_on_the_company_s_own_key_is_never_re_invoiced() {
        let own = models(
            Resolution::Company(provider()),
            Review::Runs,
            SupplyFormula::Capped,
        );
        let unserved = models(
            Resolution::Platform(None),
            Review::Runs,
            SupplyFormula::ClientKey,
        );
        for models in [own, unserved] {
            for role in ModelRole::ALL {
                assert_eq!(models.charge(role, 200_000), UsageCharge::CLIENT_KEY);
            }
        }
    }
}
