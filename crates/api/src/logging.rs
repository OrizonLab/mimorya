//! The `tracing` filter of the server. The MCP library logs its transport
//! errors with the body a server sent back, which may quote a company's
//! token: its logs are off unless the operator names it explicitly
//! (`LOG_LEVEL=info,rmcp=warn`). Connectors log their own failures, token
//! masked.

use tracing_subscriber::EnvFilter;

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
}
