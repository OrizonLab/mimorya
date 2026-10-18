//! The session cookie: read from the `Cookie` header, written as `Set-Cookie`.
//! HttpOnly and SameSite=Lax always; Secure when configured (production).

use axum::http::HeaderMap;
use axum::http::header::COOKIE;

use super::credentials::{SESSION_ABSOLUTE, SESSION_COOKIE};

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}={token}; Max-Age={}; Path=/; HttpOnly; SameSite=Lax{secure}",
        SESSION_ABSOLUTE.num_seconds()
    )
}

pub fn cleared_session_cookie(secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/; HttpOnly; SameSite=Lax{secure}"
    )
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn reads_the_session_cookie_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("theme=dark; mimorya_session=abc_-1; other=x"),
        );
        assert_eq!(session_token(&headers).as_deref(), Some("abc_-1"));
        headers.insert(COOKIE, HeaderValue::from_static("mimorya_session="));
        assert_eq!(session_token(&headers), None);
    }

    #[test]
    fn writes_protected_cookies() {
        let cookie = session_cookie("tok", true);
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.ends_with("; Secure"));
        assert!(cleared_session_cookie(false).contains("Max-Age=0"));
    }
}
