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
    assert!(file.contains("proxy_pass http://mimorya-api:4000/;"));
    let front = repository("infra/docker/edge-proxy.conf");
    assert!(front.contains("include /etc/nginx/mimorya/api-location.conf;"));
    assert!(front.contains("proxy_pass http://mimorya-web:3000;"));
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
