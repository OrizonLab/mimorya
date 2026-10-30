//! What a linked person's message to a bot asks (ADR-135, ADR-140): a
//! command of the bot's (`/nouvelle` opens a new conversation, `/aide` says
//! how to talk with it, `/start` greets them in Telegram), or words for the
//! agent, alike in Telegram and in Teams. A command is a slash and its name,
//! as Telegram writes them (a bot's username may follow it, `@…bot`), alone
//! or before the rest of the message; anything else (`/home/x`, a path) is
//! words for the agent.

/// What a person sends to leave their conversation for a new one.
pub const NEW_CONVERSATION: &str = "/nouvelle";
/// What a person sends to read how to talk with the bot.
pub const HELP: &str = "/aide";

/// The longest command name Telegram takes.
const MAX_COMMAND_CHARS: usize = 32;

/// The command `text` starts with: its name in lower case, without its
/// slash nor a bot's username, and the rest of the message, trimmed;
/// `None` when `text` is words for the agent.
pub fn command(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let named = text.strip_prefix('/')?;
    let end = named.find(char::is_whitespace).unwrap_or(named.len());
    let (word, rest) = named.split_at(end);
    let (name, bot) = match word.split_once('@') {
        Some((name, bot)) => (name, Some(bot)),
        None => (word, None),
    };
    let named_well = |part: &str, max: usize| {
        (1..=max).contains(&part.len())
            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    if !named_well(name, MAX_COMMAND_CHARS) || bot.is_some_and(|bot| !named_well(bot, 64)) {
        return None;
    }
    Some((name.to_ascii_lowercase(), rest.trim()))
}

/// What follows `/nouvelle`, if `text` asks for a new conversation: its
/// first words, or nothing.
pub fn new_conversation(text: &str) -> Option<&str> {
    let (name, rest) = command(text)?;
    (name == NEW_CONVERSATION[1..]).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_read_by_its_name() {
        assert_eq!(command("/aide"), Some(("aide".to_owned(), "")));
        assert_eq!(
            command("  /Nouvelle@NeoMaintenanceBot   Quelles visites ?  "),
            Some(("nouvelle".to_owned(), "Quelles visites ?"))
        );
        assert_eq!(command("/start"), Some(("start".to_owned(), "")));
    }

    #[test]
    fn anything_else_is_words_for_the_agent() {
        for text in [
            "Bonjour",
            "/",
            "/ aide",
            "/home/lea/rapport.pdf",
            "/aide!",
            "/aide@",
            "/aide@bot-x",
            "Voir /aide",
            &format!("/{}", "a".repeat(33)),
        ] {
            assert_eq!(command(text), None, "{text}");
        }
    }

    #[test]
    fn a_new_conversation_keeps_the_words_that_follow() {
        assert_eq!(new_conversation("/nouvelle"), Some(""));
        assert_eq!(
            new_conversation("/nouvelle Et demain ?"),
            Some("Et demain ?")
        );
        assert_eq!(new_conversation("/nouvelles"), None);
        assert_eq!(new_conversation("/aide"), None);
        assert_eq!(new_conversation("nouvelle"), None);
    }
}
