//! What the supply commands read, and what their report tells.

use chrono::{TimeZone, Utc};
use mimorya_db::repositories::model_supply::{CreditEntryRow, SupplySpending};

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
fn reads_the_shares_the_administrators_are_alerted_at() {
    let line = [
        "set-supply-alerts",
        "--slug",
        "acme",
        "--cap",
        "95,80",
        "--credit",
        "aucun",
    ];
    let alerts = SupplyChange::Alerts {
        cap: Some(vec![80, 95]),
        credit: Some(Vec::new()),
    };
    assert_eq!(parsed(&line), change("acme", alerts));
    let credit = ["set-supply-alerts", "--slug", "acme", "--credit", "10"];
    let alerts = SupplyChange::Alerts {
        cap: None,
        credit: Some(vec![10]),
    };
    assert_eq!(parsed(&credit), change("acme", alerts));
}

#[test]
fn refuses_what_it_does_not_understand() {
    let refused: [&[&str]; 10] = [
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
        &["set-supply-alerts", "--slug", "acme"],
        &["set-supply-alerts", "--slug", "acme", "--cap", "120"],
    ];
    for line in refused {
        assert!(parsed(line).is_err(), "{line:?}");
    }
}

fn report(formula: SupplyFormula, cap: Option<i64>, unlimited: bool) -> SupplyReport {
    let at = |day: u32| Utc.with_ymd_and_hms(2026, 11, day, 9, 0, 0).unwrap();
    SupplyReport {
        tenant_id: uuid::Uuid::nil(),
        formula,
        monthly_cap_micros: cap,
        unlimited,
        markup_bps: 1_250,
        cap_alerts: vec![80, 95],
        credit_alerts: Vec::new(),
        month_start: at(1) - chrono::Duration::hours(10),
        spending: SupplySpending {
            capped_cost_micros: 40_000_000,
            capped_billed_micros: 45_000_000,
            prepaid_cost_micros: 8_000_000,
            prepaid_billed_micros: 9_000_000,
            prepaid_billed_ever_micros: 120_000_000,
            credited_micros: 480_000_000,
        },
        entries: vec![
            CreditEntryRow {
                amount_micros: 500_000_000,
                reference: "FAC-2026-0042".into(),
                recorded_at: at(3),
            },
            CreditEntryRow {
                amount_micros: -20_000_000,
                reference: "AV-2026-1".into(),
                recorded_at: at(4),
            },
        ],
    }
}

#[test]
fn the_report_tells_the_real_cost_the_re_invoiced_amount_and_the_balance() {
    let prepaid = told("acme", &report(SupplyFormula::Prepaid, None, false));
    for said in [
        "Entreprise de sigle acme (identifiant 00000000-0000-0000-0000-000000000000) : \
         crédit prépayé (prepaid).",
        "Coefficient de refacturation : 12,5 %.",
        "Alertes aux administrateurs : plafond 80 %, 95 % ; solde du dernier versement \
         aucune.",
        "Depuis le 01/11/2026, sur la clé de l’exploitant :",
        "  formule capped : coût réel 40,00 USD, refacturé 45,00 USD",
        "  formule prepaid : coût réel 8,00 USD, refacturé 9,00 USD",
        "Crédit : 2 versement(s), 480,00 USD ; consommé 120,00 USD ; solde 360,00 USD.",
        "  03/11/2026  FAC-2026-0042  500,00 USD",
        "  04/11/2026  AV-2026-1  -20,00 USD",
    ] {
        assert!(prepaid.contains(said), "{said}\n{prepaid}");
    }
    assert!(!prepaid.contains("Plafond"), "{prepaid}");
    let capped = told(
        "acme",
        &report(SupplyFormula::Capped, Some(250_000_000), false),
    );
    assert!(
        capped.contains("Plafond : 250,00 USD par mois."),
        "{capped}"
    );
    let unlimited = told("acme", &report(SupplyFormula::Capped, None, true));
    assert!(
        unlimited.contains("Plafond : aucun, à la demande explicite de l’exploitant."),
        "{unlimited}"
    );
}

#[test]
fn percentages_are_told_the_french_way() {
    assert_eq!(percent(0), "0 %");
    assert_eq!(percent(1_500), "15 %");
    assert_eq!(percent(1_250), "12,5 %");
    assert_eq!(percent(725), "7,25 %");
}
