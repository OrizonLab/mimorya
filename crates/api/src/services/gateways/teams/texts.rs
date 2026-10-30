//! What the company's Teams bot says of itself (ADR-138), in the Markdown
//! Teams reads, the assistant's name escaped.

use mimorya_core::gateways::teams::escaped;

/// What a linked person is told: the agent is not talked with in Teams
/// yet; the application is where to.
pub fn not_yet(assistant_name: &str) -> String {
    format!(
        "Votre compte Teams est relié à votre compte. Les échanges avec {} dans Teams ne sont \
         pas encore ouverts : écrivez-lui dans l’application.",
        escaped(assistant_name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linked_person_is_told_where_to_talk_with_the_agent() {
        let text = not_yet("Assistant *Néo*");
        assert!(
            text.contains("avec Assistant \\*Néo\\* dans Teams"),
            "{text}"
        );
        assert!(text.ends_with("écrivez-lui dans l’application."), "{text}");
    }
}
