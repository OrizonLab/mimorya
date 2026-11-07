//! The `tracing` filter of the server. The MCP library logs its transport
//! errors with the body a server sent back, which may quote a company's
//! token: its logs are off unless the operator names it explicitly
//! (`LOG_LEVEL=info,rmcp=warn`). Connectors log their own failures, token
//! masked. Colours only for a terminal, unless `NO_COLOR` says otherwise:
//! logs kept by Docker or a collector read as plain text, an anchor copied
//! as it is written (ADR-180).

use std::ffi::OsStr;

use tracing_subscriber::EnvFilter;

/// Whether the logs are coloured: written to a terminal (`terminal`), and
/// `NO_COLOR` unset or empty (no-color.org).
pub fn log_colours(terminal: bool, no_color: Option<&OsStr>) -> bool {
    terminal && no_color.is_none_or(OsStr::is_empty)
}

pub fn log_filter(level: &str) -> anyhow::Result<EnvFilter> {
    let directives = if level.contains("rmcp") {
        level.to_string()
    } else {
        format!("{level},rmcp=off")
    };
    Ok(EnvFilter::try_new(directives)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silences_the_mcp_library_unless_named() {
        assert_eq!(log_filter("info").unwrap().to_string(), "rmcp=off,info");
        assert_eq!(
            log_filter("info,rmcp=warn").unwrap().to_string(),
            "rmcp=warn,info"
        );
        assert!(log_filter("pas un filtre[").is_err());
    }

    #[test]
    fn colours_a_terminal_only_unless_no_color_is_set() {
        assert!(log_colours(true, None));
        assert!(log_colours(true, Some(OsStr::new(""))));
        assert!(!log_colours(true, Some(OsStr::new("1"))));
        assert!(!log_colours(false, None));
        assert!(!log_colours(false, Some(OsStr::new(""))));
    }
}
