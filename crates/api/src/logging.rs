//! The `tracing` filter of the server. The MCP library logs its transport
//! errors with the body a server sent back, which may quote a company's
//! token: its logs are off unless the operator names it explicitly
//! (`LOG_LEVEL=info,rmcp=warn`). The IMAP library traces every command and
//! answer, a person's `LOGIN` password and e-mails included: its records
//! are dropped where `log` is bridged into `tracing`, before any filter
//! reads them, and its logs are off in the filter too, whatever the
//! operator names (ADR-205). Connectors log their
//! own failures, token masked. Colours only for a terminal, unless `NO_COLOR` says otherwise:
//! logs kept by Docker or a collector read as plain text, an anchor copied
//! as it is written (ADR-180).

use std::ffi::OsStr;

use tracing::level_filters::LevelFilter;
use tracing_log::AsLog;
use tracing_subscriber::EnvFilter;

/// Whether the logs are coloured: written to a terminal (`terminal`), and
/// `NO_COLOR` unset or empty (no-color.org).
pub fn log_colours(terminal: bool, no_color: Option<&OsStr>) -> bool {
    terminal && no_color.is_none_or(OsStr::is_empty)
}

/// Libraries whose logs write secrets at any level they use, never
/// enabled: a directive naming one is dropped.
const SILENCED: [&str; 2] = ["async_imap", "imap_proto"];

/// Sets the server's logs up: `tracing` filtered by `level`, coloured as
/// told, and the `log` records of the libraries bridged into it, those of
/// `SILENCED` dropped first, so that no directive (a span's included) lets
/// them through, and none finer than the filter lets through formatted at
/// all.
pub fn init_logs(level: &str, colours: bool) -> anyhow::Result<()> {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(log_filter(level)?)
        .with_ansi(colours)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    tracing_log::LogTracer::builder()
        .with_max_level(LevelFilter::current().as_log())
        .ignore_all(SILENCED)
        .init()?;
    Ok(())
}

pub fn log_filter(level: &str) -> anyhow::Result<EnvFilter> {
    let kept: Vec<&str> = level
        .split(',')
        .filter(|directive| {
            let target = directive
                .trim()
                .split(['=', '['])
                .next()
                .unwrap_or_default();
            !SILENCED.iter().any(|silenced| target.starts_with(silenced))
        })
        .collect();
    let mut directives = kept.join(",");
    if !level.contains("rmcp") {
        directives.push_str(",rmcp=off");
    }
    for silenced in SILENCED {
        directives.push_str(&format!(",{silenced}=off"));
    }
    Ok(EnvFilter::try_new(directives)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silences_the_mcp_library_unless_named() {
        let filter = log_filter("info").unwrap().to_string();
        assert!(
            filter.contains("rmcp=off") && filter.contains("info"),
            "{filter}"
        );
        let filter = log_filter("info,rmcp=warn").unwrap().to_string();
        assert!(
            filter.contains("rmcp=warn") && !filter.contains("rmcp=off"),
            "{filter}"
        );
        assert!(log_filter("pas un filtre[").is_err());
    }

    #[test]
    fn never_lets_the_imap_library_write_a_password() {
        for level in [
            "info",
            "trace",
            "info,async_imap=trace",
            "async_imap::imap_stream=trace,info",
            "info,imap_proto=debug",
        ] {
            let filter = log_filter(level).unwrap().to_string();
            assert!(filter.contains("async_imap=off"), "{level}: {filter}");
            assert!(filter.contains("imap_proto=off"), "{level}: {filter}");
            assert!(
                !filter.contains("=trace") || level == "trace",
                "{level}: {filter}"
            );
            assert!(!filter.contains("imap_stream"), "{level}: {filter}");
        }
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
