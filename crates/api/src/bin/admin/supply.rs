//! `mimorya-admin` commands that supply a company's model (ADR-208), as the
//! database's owner (`DATABASE_URL`, as `migrate`): its formula, cap,
//! markup and credits, and the report of what it consumed on the
//! operator's key, really and re-invoiced, and of its credit's balance.

use anyhow::anyhow;
use mimorya_api::services::provisioning::{
    CapChoice, SupplyChange, SupplyReport, change_model_supply, model_supply_report,
};
use mimorya_core::model_supply::{SupplyFormula, format_usd, parse_percent, parse_usd};
use mimorya_core::time_zone::PLATFORM_TIME_ZONE;
use sqlx::postgres::PgPoolOptions;

use super::args::Options;

/// The commands this module reads.
pub const COMMANDS: [&str; 4] = [
    "model-supply",
    "set-model-supply",
    "set-markup",
    "add-credit",
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
mod tests {
    use super::*;

    fn parsed(line: &[&str]) -> Result<SupplyCommand, String> {
        let (command, rest) = line.split_first().unwrap();
        let rest: Vec<String> = rest.iter().map(|arg| (*arg).to_string()).collect();
        let mut options = Options::read(&rest)?;
        let parsed = parse(command, &mut options)?;
        options.finish()?;
        Ok(parsed)
    }

    fn change(slug: &str, change: SupplyChange) -> Result<SupplyCommand, String> {
        Ok(SupplyCommand::Change {
            slug: slug.into(),
            change,
        })
    }

    #[test]
    fn reads_each_command_of_the_supply() {
        assert_eq!(
            parsed(&["model-supply", "--slug", "acme"]),
            Ok(SupplyCommand::Show {
                slug: "acme".into()
            })
        );
        for (cap, chosen) in [
            (None, CapChoice::Default),
            (Some("250,50"), CapChoice::Amount(250_500_000)),
            (Some("unlimited"), CapChoice::Unlimited),
        ] {
            let mut line = vec!["set-model-supply", "--slug", "acme", "--formula", "capped"];
            if let Some(cap) = cap {
                line.extend(["--cap", cap]);
            }
            assert_eq!(
                parsed(&line),
                change(
                    "acme",
                    SupplyChange::Formula {
                        formula: SupplyFormula::Capped,
                        cap: chosen
                    }
                )
            );
        }
        assert_eq!(
            parsed(&["set-markup", "--percent", "12,5", "--slug", "acme"]),
            change("acme", SupplyChange::Markup { bps: 1_250 })
        );
        assert_eq!(
            parsed(&[
                "add-credit",
                "--slug",
                "acme",
                "--amount",
                "-20",
                "--reference",
                "AV-2026-0003"
            ]),
            change(
                "acme",
                SupplyChange::Credit {
                    amount_micros: -20_000_000,
                    reference: "AV-2026-0003".into()
                }
            )
        );
    }

    #[test]
    fn refuses_what_it_does_not_understand() {
        let refused: [&[&str]; 8] = [
            &["model-supply"],
            &["set-model-supply", "--slug", "acme"],
            &["set-model-supply", "--slug", "acme", "--formula", "plafond"],
            &[
                "set-model-supply",
                "--slug",
                "a",
                "--formula",
                "capped",
                "--cap",
                "beaucoup",
            ],
            &["set-markup", "--slug", "acme", "--percent", "-5"],
            &["add-credit", "--slug", "acme", "--amount", "500"],
            &[
                "add-credit",
                "--slug",
                "acme",
                "--amount",
                "x",
                "--reference",
                "F",
            ],
            &["model-supply", "--slug", "acme", "--formula", "capped"],
        ];
        for line in refused {
            assert!(parsed(line).is_err(), "{line:?}");
        }
    }

    #[test]
    fn percentages_are_told_the_french_way() {
        assert_eq!(percent(0), "0 %");
        assert_eq!(percent(1_500), "15 %");
        assert_eq!(percent(1_250), "12,5 %");
        assert_eq!(percent(725), "7,25 %");
    }
}
