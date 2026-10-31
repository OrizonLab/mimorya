//! The conversation gateways a company declares (ADR-125): an application
//! of a messaging service through which its people talk with the agent
//! there, each as themselves once they linked their identity on that
//! service to their account. Slack first (`slack`): what a company declares
//! of its application, how an event Slack sends proves it comes from it,
//! and which of its messages reach the agent; then Telegram (`telegram`,
//! ADR-133): its bot, the secret its updates prove themselves by, and the
//! code a person links their account with; then Microsoft Teams (`teams`,
//! ADR-138): its Azure bot, the Bot Framework token its activities prove
//! themselves by, and when Teams proves a person. What a person's message
//! to a bot asks, a command or words for the agent, is read alike in
//! Telegram and in Teams (`command`, ADR-140).

mod command;
mod markdown;
pub mod slack;
pub mod teams;
pub mod telegram;
mod validation;

pub use command::{HELP, NEW_CONVERSATION, command, new_conversation};
pub use markdown::bounded;
pub use validation::{
    ActionState, MAX_ACTION_CHARS, MAX_PARAMETERS_CHARS, ShownAction, exact_parameters, risk_label,
    to_write_again, validated_in_gateway,
};

/// A gateway's name, as kept: bounded like the column.
pub const MIN_NAME_CHARS: usize = 2;
pub const MAX_NAME_CHARS: usize = 80;

/// The services a gateway reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayKind {
    Slack,
    Telegram,
    Teams,
}

impl GatewayKind {
    /// As stored and sent.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Telegram => "telegram",
            Self::Teams => "teams",
        }
    }

    /// As stored, if known.
    pub fn of(kind: &str) -> Option<Self> {
        match kind {
            "slack" => Some(Self::Slack),
            "telegram" => Some(Self::Telegram),
            "teams" => Some(Self::Teams),
            _ => None,
        }
    }

    /// Its name, in French sentences.
    pub fn product(self) -> &'static str {
        match self {
            Self::Slack => "Slack",
            Self::Telegram => "Telegram",
            Self::Teams => "Teams",
        }
    }
}

/// A gateway's name once trimmed, or why it cannot be one (in French).
pub fn gateway_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    let chars = name.chars().count();
    if !(MIN_NAME_CHARS..=MAX_NAME_CHARS).contains(&chars) {
        return Err("Nom de la passerelle : 2 à 80 caractères");
    }
    if name.chars().any(char::is_control) {
        return Err("Nom de la passerelle : caractères de contrôle refusés");
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        assert_eq!(gateway_name("  Slack Néo  ").as_deref(), Ok("Slack Néo"));
        assert!(gateway_name("S").is_err());
        assert!(gateway_name(&"é".repeat(81)).is_err());
        assert!(gateway_name("Slack\u{0007}").is_err());
        assert_eq!(
            gateway_name(&"é".repeat(80)).map(|n| n.chars().count()),
            Ok(80)
        );
    }

    #[test]
    fn a_kind_reads_back_as_written() {
        assert_eq!(GatewayKind::of("slack"), Some(GatewayKind::Slack));
        assert_eq!(
            GatewayKind::of(GatewayKind::Slack.as_str()),
            Some(GatewayKind::Slack)
        );
        assert_eq!(GatewayKind::of("Slack"), None);
        assert_eq!(GatewayKind::of("discord"), None);
        assert_eq!(
            GatewayKind::of(GatewayKind::Teams.as_str()),
            Some(GatewayKind::Teams)
        );
        assert_eq!(
            GatewayKind::of(GatewayKind::Telegram.as_str()),
            Some(GatewayKind::Telegram)
        );
    }
}
