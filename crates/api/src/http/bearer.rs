//! The token another system sends as `Authorization: Bearer <token>`, before
//! any session: a delivery to an inbound hook (ADR-109), a SIEM reading its
//! company's audit log (ADR-181).

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;

/// The token sent as `Authorization: Bearer <token>`, if any.
pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn sent(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn reads_a_bearer_token_only() {
        assert_eq!(bearer(&sent("Bearer mae_abc")), Some("mae_abc"));
        assert_eq!(bearer(&sent("bearer  mae_abc ")), Some("mae_abc"));
        assert_eq!(bearer(&sent("Basic mae_abc")), None);
        assert_eq!(bearer(&sent("Bearer ")), None);
        assert_eq!(bearer(&sent("mae_abc")), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }
}
