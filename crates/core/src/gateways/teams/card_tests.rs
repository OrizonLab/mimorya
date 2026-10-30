use serde_json::{Value, json};
use uuid::Uuid;

use super::*;

fn shown<'a>(id: u128, summary: &'a str, risk: &'a str, parameters: &'a Value) -> ShownAction<'a> {
    ShownAction {
        approval_id: Uuid::from_u128(id),
        summary,
        risk,
        parameters,
    }
}

/// Every text the card shows, in order.
fn texts(card: &Value) -> Vec<String> {
    let mut texts = Vec::new();
    for block in card["body"].as_array().unwrap() {
        for inline in block["inlines"].as_array().into_iter().flatten() {
            assert_eq!(inline["type"], "TextRun", "{inline}");
            texts.push(inline["text"].as_str().unwrap().to_owned());
        }
    }
    texts
}

/// The buttons the card shows: their title and what they carry.
fn buttons(card: &Value) -> Vec<(String, Value)> {
    let blocks = card["body"].as_array().unwrap();
    (blocks.iter())
        .filter(|block| block["type"] == "ActionSet")
        .flat_map(|block| block["actions"].as_array().unwrap().clone())
        .map(|button| {
            assert_eq!(button["type"], "Action.Submit");
            (
                button["title"].as_str().unwrap().to_owned(),
                button["data"].clone(),
            )
        })
        .collect()
}

#[test]
fn only_actions_that_may_be_validated_there_are_shown_as_many_as_a_card_holds() {
    let small = json!({ "proposalId": "p-1" });
    let large = json!({ "body": "x".repeat(2_000) });
    let actions = [
        shown(1, "Envoyer", "external", &small),
        shown(2, "Planifier", "write", &small),
        shown(3, "Long", "write", &large),
        shown(4, "Encore long", "write", &large),
    ];
    let ids: Vec<u128> = (shown_in_card(actions).iter())
        .map(|a| a.approval_id.as_u128())
        .collect();
    // The external one never; the second large one no longer fits.
    assert_eq!(ids, [2, 3]);
    let many: Vec<ShownAction> = (1..=8)
        .map(|i| shown(i, "Planifier", "write", &small))
        .collect();
    assert_eq!(shown_in_card(many).len(), MAX_CARD_ACTIONS);
}

#[test]
fn a_card_shows_each_action_whole_as_plain_text_with_its_buttons() {
    let parameters = json!({ "note": "[clic](https://evil.example) **gras** <b>" });
    let one = shown(7, "Planifier *INT-2052* [ici](x)", "write", &parameters);
    let card = validation_card(&[CardAction {
        shown: one,
        open: true,
    }]);
    assert_eq!(card["type"], "AdaptiveCard");
    let texts = texts(&card);
    assert_eq!(texts[0], "Action à valider : ");
    // As the agent wrote it, in a run Teams never reads as Markdown.
    assert_eq!(texts[1], "Planifier *INT-2052* [ici](x)");
    assert_eq!(
        texts[3],
        "Modification. Paramètres exacts qui seront exécutés :"
    );
    assert_eq!(texts[4], serde_json::to_string_pretty(&parameters).unwrap());
    let approval = Uuid::from_u128(7).to_string();
    assert_eq!(
        buttons(&card),
        [
            (
                "Valider".to_owned(),
                json!({ "mimorya": "v", "approval": approval })
            ),
            (
                "Refuser".to_owned(),
                json!({ "mimorya": "r", "approval": approval })
            ),
        ]
    );
}

#[test]
fn several_actions_are_numbered_a_decided_one_shown_without_its_buttons() {
    let small = json!({ "proposalId": "p-1" });
    let card = validation_card(&[
        CardAction {
            shown: shown(1, "Première", "write", &small),
            open: false,
        },
        CardAction {
            shown: shown(2, "Seconde", "write", &small),
            open: true,
        },
    ]);
    let texts = texts(&card);
    assert_eq!(texts[0], "Action 1 à valider : ");
    assert!(texts.contains(&"Décidée.".to_owned()), "{texts:?}");
    assert!(
        texts.contains(&"Action 2 à valider : ".to_owned()),
        "{texts:?}"
    );
    let titles: Vec<String> = buttons(&card).into_iter().map(|(title, _)| title).collect();
    assert_eq!(titles, ["Valider 2", "Refuser 2"]);
}

#[test]
fn a_pressed_button_is_read_only_as_the_card_writes_it() {
    let approval = Uuid::parse_str("0d6f8a1b-2c3d-4e5f-8a9b-0c1d2e3f4a5b").unwrap();
    let id = approval.to_string();
    assert_eq!(
        card_choice(&json!({ "mimorya": "v", "approval": id })),
        Some(CardChoice {
            approval_id: approval,
            approved: true
        })
    );
    assert_eq!(
        card_choice(&json!({ "mimorya": "r", "approval": id, "msteams": {} })),
        Some(CardChoice {
            approval_id: approval,
            approved: false
        })
    );
    for value in [
        json!({ "mimorya": "x", "approval": id }),
        json!({ "mimorya": "v" }),
        json!({ "mimorya": "v", "approval": id.to_uppercase() }),
        json!({ "mimorya": "v", "approval": "pas-un-uuid" }),
        json!({ "approval": id }),
        json!("v"),
        Value::Null,
    ] {
        assert_eq!(card_choice(&value), None, "{value}");
    }
}
