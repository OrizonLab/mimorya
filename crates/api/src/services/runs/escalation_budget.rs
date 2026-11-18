//! What a company's model escalation may still spend this month (ADR-207),
//! read before each escalated call: what its mechanisms' calls cost since
//! the start of the month, against its budget. Runs under way at once see
//! each other's calls only once they are recorded: the budget may be
//! passed by those calls in flight, no more.

use async_trait::async_trait;
use mimorya_agent::runtime::EscalationBudget;
use mimorya_db::Database;
use uuid::Uuid;

use crate::services::llm::month_spending;

pub struct MonthlyBudget {
    db: Database,
    tenant_id: Uuid,
    budget_micros: i64,
}

impl MonthlyBudget {
    pub fn new(db: Database, tenant_id: Uuid, budget_micros: i64) -> Self {
        Self {
            db,
            tenant_id,
            budget_micros,
        }
    }
}

#[async_trait]
impl EscalationBudget for MonthlyBudget {
    async fn allows(&self) -> anyhow::Result<bool> {
        let mut tx = self.db.begin(self.tenant_id).await?;
        let spending = month_spending(&mut tx).await?;
        tx.commit().await?;
        Ok(spending.spent_micros < self.budget_micros)
    }
}
