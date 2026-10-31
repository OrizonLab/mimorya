//! What a message of the bot says once none of the actions it shows awaits
//! a decision any more, alike in Teams (ADR-141, ADR-142) and in Telegram
//! (ADR-148), each service writing it in its own markup: each validated or
//! refused (`decided`), or one expired, or stopped with its request; one
//! action shown (`one`), or several.

/// A pause's, before where its conversation is.
pub fn settled_pause(decided: bool, one: bool) -> &'static str {
    match (decided, one) {
        (true, true) => "L’action de cette demande montrée ci-dessous est décidée",
        (true, false) => "Les actions de cette demande montrées ci-dessous sont décidées",
        (false, true) => "L’action de cette demande montrée ci-dessous n’attend plus de décision",
        (false, false) => {
            "Les actions de cette demande montrées ci-dessous n’attendent plus de décision"
        }
    }
}

/// A notice's, after whom the agent acts for.
pub fn settled_notice(decided: bool, one: bool) -> &'static str {
    match (decided, one) {
        (true, true) => "l’action montrée ci-dessous est décidée",
        (true, false) => "les actions montrées ci-dessous sont décidées",
        (false, true) => "l’action montrée ci-dessous n’attend plus de décision",
        (false, false) => "les actions montrées ci-dessous n’attendent plus de décision",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settled_message_says_whether_its_actions_are_decided_and_how_many() {
        assert!(settled_pause(true, true).starts_with("L’action"));
        assert!(settled_pause(true, false).ends_with("sont décidées"));
        assert!(settled_pause(false, false).ends_with("n’attendent plus de décision"));
        assert_eq!(
            settled_notice(false, true),
            "l’action montrée ci-dessous n’attend plus de décision"
        );
        assert!(settled_notice(true, false).starts_with("les actions"));
    }
}
