//! The record of processing activities (RGPD art. 30, ADR-217) the platform
//! keeps for each company, its controller: what it processes on the
//! company's behalf, for what, about whom, sent to whom and kept how long,
//! with a general description of its security measures. It is drawn from
//! the company's settings each time it is asked for, never stored, so it
//! never lags behind them. The platform states what it knows; what the
//! company alone knows (its legal bases, its data protection officer, the
//! guarantees of each transfer) is listed for it to complete.

mod activities;
mod facts;
mod measures;
mod model_recipients;
mod recipients;
mod retention;

#[cfg(test)]
mod tests;

pub use activities::{Activity, ActivityKind};
pub use facts::{
    ChosenBy, ConnectorFact, ConnectorKind, DecisionUses, IdentityFact, KeptData, MailRelay,
    ModelService, RegisterFacts, RetentionFact,
};
pub use recipients::{Place, Recipient};

/// The register of a company.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Register {
    /// The controller: the company.
    pub controller: String,
    pub activities: Vec<Activity>,
    pub measures: Vec<String>,
    /// What the company completes itself.
    pub to_complete: Vec<&'static str>,
}

pub fn register(facts: &RegisterFacts) -> Register {
    Register {
        controller: facts.company.clone(),
        activities: activities::activities(facts),
        measures: measures::measures(facts),
        to_complete: measures::TO_COMPLETE.to_vec(),
    }
}
