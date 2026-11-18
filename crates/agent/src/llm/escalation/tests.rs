use super::*;
use crate::llm::PriceTable;

fn haiku() -> TenantLlmChoice {
    TenantLlmChoice {
        provider: "anthropic".into(),
        model: "claude-haiku-5-5".into(),
        base_url: None,
    }
}

fn request(model: &str) -> EscalationRequest<'_> {
    EscalationRequest {
        model,
        after_failure: false,
        router: false,
        advisor: false,
        monthly_budget_micros: 50_000_000,
        max_per_run: 5,
    }
}

#[test]
fn a_model_of_the_conversations_provider_is_kept_with_its_provider_and_endpoint() {
    let choice = check_escalation(
        Some(&haiku()),
        &request("  claude-opus-5-5 "),
        &PriceTable::default(),
    )
    .unwrap();
    assert_eq!(
        choice,
        EscalationChoice {
            provider: "anthropic".into(),
            base_url: None,
            model: "claude-opus-5-5".into(),
            after_failure: false,
            router: false,
            advisor: false,
            monthly_budget_micros: 50_000_000,
            max_per_run: 5,
        }
    );
    assert_eq!(
        escalation_problem(&choice, Some(&haiku()), &PriceTable::default()),
        None
    );
}

#[test]
fn a_company_on_the_platform_s_model_may_not_choose_one() {
    let refused =
        check_escalation(None, &request("claude-opus-5-5"), &PriceTable::default()).unwrap_err();
    assert!(refused.contains("fournisseur propre"), "{refused}");
}

#[test]
fn the_model_is_named_well_and_differs_from_the_main_one() {
    for (model, said) in [
        ("", "Indiquez le modèle d’escalade"),
        ("claude opus", "Nom de modèle invalide"),
        ("claude-haiku-5-5", "doit différer du modèle principal"),
    ] {
        let refused =
            check_escalation(Some(&haiku()), &request(model), &PriceTable::default()).unwrap_err();
        assert!(refused.contains(said), "{model}: {refused}");
    }
}

#[test]
fn a_dated_identifier_of_the_main_model_is_the_main_model() {
    let prices = PriceTable::default();
    let dated = "claude-haiku-5-5-20261101";
    assert!(prices.has(dated));
    let refused = check_escalation(Some(&haiku()), &request(dated), &prices).unwrap_err();
    assert!(
        refused.contains("doit différer du modèle principal"),
        "{refused}"
    );
    // Stored before the conversations moved to a dated identifier of it.
    let stored = check_escalation(Some(&haiku()), &request("claude-opus-5-5"), &prices).unwrap();
    let opus = TenantLlmChoice {
        model: "claude-opus-5-5-20261101".into(),
        ..haiku()
    };
    let problem = escalation_problem(&stored, Some(&opus), &prices).unwrap();
    assert!(
        problem.contains("doit différer du modèle principal"),
        "{problem}"
    );
}

#[test]
fn the_budget_and_the_escalations_per_run_keep_to_their_bounds() {
    for (budget, per_run, said) in [
        (999_999, 5, "1 à 10 000 USD"),
        (10_000_000_001, 5, "1 à 10 000 USD"),
        (1_000_000, 0, "1 à 50"),
        (1_000_000, 51, "1 à 50"),
    ] {
        let asked = EscalationRequest {
            monthly_budget_micros: budget,
            max_per_run: per_run,
            ..request("claude-opus-5-5")
        };
        let refused = check_escalation(Some(&haiku()), &asked, &PriceTable::default()).unwrap_err();
        assert!(refused.contains(said), "{budget}, {per_run}: {refused}");
    }
    let widest = EscalationRequest {
        monthly_budget_micros: MAX_ESCALATION_BUDGET_MICROS,
        max_per_run: MAX_ESCALATIONS_PER_RUN,
        ..request("claude-opus-5-5")
    };
    assert!(check_escalation(Some(&haiku()), &widest, &PriceTable::default()).is_ok());
}

#[test]
fn a_mechanism_not_delivered_yet_is_refused_rather_than_kept_without_effect() {
    for (asked, label) in [
        (
            EscalationRequest {
                after_failure: true,
                ..request("claude-opus-5-5")
            },
            Mechanism::AfterFailure,
        ),
        (
            EscalationRequest {
                router: true,
                ..request("claude-opus-5-5")
            },
            Mechanism::Router,
        ),
        (
            EscalationRequest {
                advisor: true,
                ..request("claude-opus-5-5")
            },
            Mechanism::Advisor,
        ),
    ] {
        let result = check_escalation(Some(&haiku()), &asked, &PriceTable::default());
        if label.available() {
            assert!(result.is_ok(), "{label:?}");
        } else {
            let refused = result.unwrap_err();
            assert!(refused.contains("pas encore disponible"), "{refused}");
            assert!(refused.contains(label.label()), "{refused}");
        }
    }
}

#[test]
fn a_choice_stops_serving_once_the_conversations_provider_changes() {
    let choice = check_escalation(
        Some(&haiku()),
        &request("claude-opus-5-5"),
        &PriceTable::default(),
    )
    .unwrap();
    let moved = TenantLlmChoice {
        provider: "openai_compatible".into(),
        model: "mistral-large".into(),
        base_url: Some("http://llm.interne/v1".into()),
    };
    assert!(
        escalation_problem(&choice, Some(&moved), &PriceTable::default())
            .unwrap()
            .contains("a changé")
    );
    assert!(
        escalation_problem(&choice, None, &PriceTable::default())
            .unwrap()
            .contains("fournisseur propre")
    );
    let same = TenantLlmChoice {
        model: "claude-opus-5-5".into(),
        ..haiku()
    };
    assert!(
        escalation_problem(&choice, Some(&same), &PriceTable::default())
            .unwrap()
            .contains("doit différer")
    );
}

#[test]
fn a_model_without_a_known_price_is_refused_the_budget_could_not_bound_it() {
    let refused = check_escalation(
        Some(&haiku()),
        &request("claude-sans-prix"),
        &PriceTable::default(),
    )
    .unwrap_err();
    assert!(refused.contains("ne connaît pas le prix"), "{refused}");
}

#[test]
fn a_stored_model_that_lost_its_price_stops_serving() {
    let mut stored = check_escalation(
        Some(&haiku()),
        &request("claude-opus-5-5"),
        &PriceTable::default(),
    )
    .unwrap();
    stored.model = "claude-retire".into();
    assert!(
        escalation_problem(&stored, Some(&haiku()), &PriceTable::default())
            .unwrap()
            .contains("ne connaît plus le prix")
    );
}
