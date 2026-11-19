//! `mimorya-admin` commands that supply a company's model (ADR-208), as the
//! database's owner (`DATABASE_URL`, as `migrate`): its formula, cap,
//! markup, credits and the shares its administrators are alerted at
//! (ADR-211), and the report of what it consumed on the operator's key,
//! really and re-invoiced, and of its credit's balance.

use anyhow::anyhow;
use mimorya_api::services::provisioning::{
    CapChoice, SupplyChange, SupplyReport, change_model_supply, model_supply_report,
};
use mimorya_core::model_supply::{
    AlertKind, SupplyFormula, format_usd, parse_alerts, parse_percent, parse_usd,
};
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use sqlx::postgres::PgPoolOptions;

use super::args::Options;

/// The commands this module reads.
pub const COMMANDS: [&str; 5] = [
    "model-supply",
    "set-model-supply",
    "set-markup",
    "add-credit",
    "set-supply-alerts",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplyCommand {
    Show { slug: String },
    Change { slug: String, change: SupplyChange },
}

/// `command`, one of `COMMANDS`, read with its options.
pub fn parse(command: &str, options: &mut Options) -> Result<SupplyCommand, String> {
    let slug = options.take("--slug")?;
    let change = match command {
        "model-supply" => return Ok(SupplyCommand::Show { slug }),
        "set-model-supply" => {
            let written = options.take("--formula")?;
            let formula = SupplyFormula::from_command(&written).ok_or_else(|| {
                format!("Formule inconnue : {written} (client-key, capped ou prepaid)")
            })?;
            let cap = match options.take_optional("--cap") {
                None => CapChoice::Default,
                Some(cap) if cap.trim() == "unlimited" => CapChoice::Unlimited,
                Some(cap) => CapChoice::Amount(parse_usd(&cap, false)?),
            };
            SupplyChange::Formula { formula, cap }
        }
        "set-markup" => SupplyChange::Markup {
            bps: parse_percent(&options.take("--percent")?)?,
        },
        "add-credit" => SupplyChange::Credit {
            amount_micros: parse_usd(&options.take("--amount")?, true)?,
            reference: options.take("--reference")?,
        },
        "set-supply-alerts" => {
            let cap = options.take_optional("--cap");
            let credit = options.take_optional("--credit");
            if cap.is_none() && credit.is_none() {
                return Err(
                    "Indiquez --cap, --credit ou les deux (« 80,95 », ou « aucun »)".into(),
                );
            }
            SupplyChange::Alerts {
                cap: cap
                    .map(|cap| parse_alerts(AlertKind::Cap, &cap))
                    .transpose()?,
                credit: credit
                    .map(|credit| parse_alerts(AlertKind::Credit, &credit))
                    .transpose()?,
            }
        }
        other => return Err(format!("Commande inconnue : {other}")),
    };
    Ok(SupplyCommand::Change { slug, change })
}

pub async fn run(command: SupplyCommand) -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow!("DATABASE_URL (rôle propriétaire) est obligatoire"))?;
    let owner = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let (slug, report) = match command {
        SupplyCommand::Show { slug } => {
            let report = model_supply_report(&owner, &slug).await;
            (slug, report)
        }
        SupplyCommand::Change { slug, change } => {
            let report = change_model_supply(&owner, &slug, change).await;
            if report.is_ok() {
                println!("Enregistré, et tracé dans le journal d’audit de l’entreprise.");
            }
            (slug, report)
        }
    };
    owner.close().await;
    println!("{}", told(slug.trim(), &report?));
    Ok(())
}

/// The company's supply, told to the operator.
pub fn told(slug: &str, report: &SupplyReport) -> String {
    let mut lines = vec![format!(
        "Entreprise de sigle {slug} (identifiant {}) : {} ({}).",
        report.tenant_id,
        report.formula.label(),
        report.formula.as_str()
    )];
    match (report.formula, report.monthly_cap_micros) {
        (SupplyFormula::Capped, Some(cap)) => {
            lines.push(format!("Plafond : {} par mois.", format_usd(cap)));
        }
        (SupplyFormula::Capped, None) if report.unlimited => {
            lines.push("Plafond : aucun, à la demande explicite de l’exploitant.".into());
        }
        _ => {}
    }
    lines.push(format!(
        "Coefficient de refacturation : {}.",
        percent(report.markup_bps)
    ));
    lines.push(format!(
        "Alertes aux administrateurs : plafond {} ; solde du dernier versement {}.",
        shares(&report.cap_alerts),
        shares(&report.credit_alerts)
    ));
    let spent = &report.spending;
    lines.push(format!(
        "Depuis le {}, sur la clé de l’exploitant :",
        report
            .month_start
            .with_timezone(&PLATFORM_TIME_ZONE)
            .format("%d/%m/%Y")
    ));
    lines.push(format!(
        "  formule capped : coût réel {}, refacturé {}",
        format_usd(spent.capped_cost_micros),
        format_usd(spent.capped_billed_micros)
    ));
    lines.push(format!(
        "  formule prepaid : coût réel {}, refacturé {}",
        format_usd(spent.prepaid_cost_micros),
        format_usd(spent.prepaid_billed_micros)
    ));
    lines.push(format!(
        "Crédit : {} versement(s), {} ; consommé {} ; solde {}.",
        report.entries.len(),
        format_usd(spent.credited_micros),
        format_usd(spent.prepaid_billed_ever_micros),
        format_usd(spent.balance_micros())
    ));
    for entry in &report.entries {
        lines.push(format!(
            "  {}  {}  {}",
            entry
                .recorded_at
                .with_timezone(&PLATFORM_TIME_ZONE)
                .format("%d/%m/%Y"),
            entry.reference,
            format_usd(entry.amount_micros)
        ));
    }
    lines.join("\n")
}

/// Shares in percent, or none.
fn shares(percents: &[u8]) -> String {
    match percents {
        [] => "aucune".into(),
        _ => percents
            .iter()
            .map(|percent| format!("{percent} %"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Basis points as a percentage, the French way.
fn percent(bps: u32) -> String {
    let (units, hundredths) = (bps / 100, bps % 100);
    match hundredths {
        0 => format!("{units} %"),
        h if h.is_multiple_of(10) => format!("{units},{} %", h / 10),
        h => format!("{units},{h:02} %"),
    }
}

#[cfg(test)]
#[path = "supply/tests.rs"]
mod tests;
