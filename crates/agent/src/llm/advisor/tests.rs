use super::*;

#[test]
fn a_model_advises_the_models_anthropic_lists_for_it() {
    assert!(advisor_pair_accepted("claude-haiku-5-5", "claude-opus-5-5"));
    assert!(advisor_pair_accepted(
        "claude-sonnet-5-5",
        "claude-opus-5-5"
    ));
    // Dated identifiers read as their alias.
    assert!(advisor_pair_accepted(
        "claude-haiku-4-5-20251001",
        "claude-sonnet-4-6"
    ));
    // A weaker or unlisted advisor is refused.
    assert!(!advisor_pair_accepted(
        "claude-opus-5-5",
        "claude-haiku-5-5"
    ));
    assert!(!advisor_pair_accepted(
        "claude-sonnet-5-5",
        "claude-opus-4-8"
    ));
    assert!(!advisor_pair_accepted(
        "claude-fable-5-2",
        "claude-fable-5-1"
    ));
    assert!(!advisor_pair_accepted("gpt-5", "claude-opus-5-5"));
    assert!(advisors_for("mistral-large").is_empty());
}

/// A run that escalates keeps its tools unchanged: the escalation model then
/// calls with the same advisor, itself, which Anthropic must accept.
#[test]
fn every_advisor_may_advise_itself() {
    for (_, advisors) in ADVISOR_PAIRS {
        for advisor in *advisors {
            assert!(advisor_pair_accepted(advisor, advisor), "{advisor}");
        }
    }
}

#[test]
fn the_offer_says_why_not_in_french() {
    let open = AdvisorOffer {
        operator_allows: true,
        anthropic_official: true,
    };
    assert_eq!(open.problem(), None);
    assert_eq!(
        open.pair_problem("claude-haiku-5-5", "claude-opus-5-5"),
        None
    );
    let closed = AdvisorOffer {
        operator_allows: false,
        ..open
    };
    assert!(closed.problem().unwrap().contains("n’a pas ouvert"));
    let elsewhere = AdvisorOffer {
        anthropic_official: false,
        ..open
    };
    assert!(elsewhere.problem().unwrap().contains("adresse officielle"));
    assert!(
        elsewhere
            .pair_problem("claude-haiku-5-5", "claude-opus-5-5")
            .unwrap()
            .contains("adresse officielle")
    );
    let refused = open
        .pair_problem("claude-sonnet-5-5", "claude-opus-4-8")
        .unwrap();
    assert!(refused.contains("claude-opus-5-5"), "{refused}");
    let none = open.pair_problem("gpt-5", "claude-opus-5-5").unwrap();
    assert!(none.contains("pas de conseiller"), "{none}");
}

#[test]
fn a_run_s_tool_is_cached_from_three_consultations() {
    let tool = advisor_tool("claude-opus-5-5", 2);
    assert_eq!((tool.max_uses, tool.cache), (2, false));
    let tool = advisor_tool("claude-opus-5-5", 3);
    assert_eq!(
        tool,
        AdvisorTool {
            model: "claude-opus-5-5".into(),
            max_uses: 3,
            cache: true
        }
    );
}
