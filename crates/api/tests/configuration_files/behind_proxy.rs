//! The stack behind a reverse proxy already on the server (ADR-225): the
//! block the guide gives Nginx Proxy Manager is the very file the CI stands
//! in front of the stack, and the names the guide gives are the stack's.

use super::stack::repository;

/// The lines of an nginx block, without its comments and blank lines.
fn lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[test]
fn the_guide_gives_the_api_block_the_ci_stands_in_front_of_the_stack() {
    let guide = repository("docs/INSTALLATION.md");
    let given = guide
        .split("```nginx\n")
        .skip(1)
        .filter_map(|rest| rest.split_once("```").map(|(code, _)| code))
        .find(|code| code.contains("location /api/"))
        .expect("the guide gives the API's block");
    let file = repository("infra/proxy/api-location.conf");
    assert_eq!(lines(given), lines(&file), "docs/INSTALLATION.md");
    let front = repository("infra/docker/edge-proxy.conf");
    assert!(front.contains("include /etc/nginx/mimorya/api-location.conf;"));
    // The names are resolved at each request: an nginx that resolved them as
    // it loads would refuse to start, taking its other sites down, while the
    // stack is stopped.
    for (conf, name) in [(&file, "mimorya-api:4000"), (&front, "mimorya-web")] {
        assert!(conf.contains("resolver 127.0.0.11 valid=10s ipv6=off;"));
        assert!(
            conf.contains(&format!(" {name};")),
            "{name} set in a variable"
        );
        let literal = lines(conf)
            .into_iter()
            .find(|line| line.starts_with("proxy_pass") && line.contains("mimorya-"));
        assert_eq!(literal, None, "a name written in proxy_pass");
    }
}

#[test]
fn the_guide_names_the_stack_s_network_and_names() {
    let stack = repository("compose.behind-proxy.yml");
    for written in [
        "profiles: [caddy]",
        "name: mimorya-edge",
        "aliases: [mimorya-web]",
        "aliases: [mimorya-api]",
    ] {
        assert!(
            stack.contains(written),
            "compose.behind-proxy.yml: {written}"
        );
    }
    let guide = repository("docs/INSTALLATION.md");
    for named in [
        "COMPOSE_FILE=compose.production.yml:compose.behind-proxy.yml",
        "docker network create mimorya-edge",
        "`mimorya-web`",
        "`mimorya-api`",
        "MIMORYA_TLS=internal",
    ] {
        assert!(guide.contains(named), "docs/INSTALLATION.md: {named}");
    }
}
