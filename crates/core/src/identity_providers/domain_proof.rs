//! Proving an e-mail domain (ADR-069): the company publishes, in the
//! domain's DNS, a TXT record of its own under `_mimorya.<domain>`. Its
//! value is derived from the company and the domain, so that one company's
//! record never proves the domain for another. It is not secret: only whoever
//! controls the domain's DNS can publish it.

use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The prefix of the record's value.
const VALUE_PREFIX: &str = "mimorya-verification=";

/// Where the record is published.
pub fn proof_record_name(domain: &str) -> String {
    format!("_mimorya.{}", domain.to_ascii_lowercase())
}

/// The value the record carries for this company and domain.
pub fn proof_record_value(tenant_id: Uuid, domain: &str) -> String {
    let seed = format!(
        "mimorya-domain-proof:{tenant_id}:{}",
        domain.to_ascii_lowercase()
    );
    let digest = Sha256::digest(seed.as_bytes());
    let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("{VALUE_PREFIX}{hex}")
}

/// Whether one of the TXT values found at the record's name proves the
/// domain for the company.
pub fn proves_domain(tenant_id: Uuid, domain: &str, found: &[String]) -> bool {
    let expected = proof_record_value(tenant_id, domain);
    found.iter().any(|value| value.trim() == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEO: Uuid = Uuid::from_u128(1);
    const OTHER: Uuid = Uuid::from_u128(2);

    #[test]
    fn the_record_is_the_company_s_own_for_this_domain() {
        let value = proof_record_value(NEO, "Neo.example");
        assert_eq!(proof_record_name("Neo.example"), "_mimorya.neo.example");
        assert_eq!(value, proof_record_value(NEO, "neo.example"));
        assert!(value.starts_with(VALUE_PREFIX) && value.len() == VALUE_PREFIX.len() + 32);
        assert_ne!(value, proof_record_value(OTHER, "neo.example"));
        assert_ne!(value, proof_record_value(NEO, "autre.example"));
    }

    #[test]
    fn only_the_company_s_value_proves_the_domain() {
        let ours = proof_record_value(NEO, "neo.example");
        let theirs = proof_record_value(OTHER, "neo.example");
        assert!(proves_domain(
            NEO,
            "neo.example",
            &[theirs.clone(), format!(" {ours} ")]
        ));
        assert!(!proves_domain(NEO, "neo.example", &[theirs]));
        assert!(!proves_domain(NEO, "neo.example", &[format!("{ours}x")]));
        assert!(!proves_domain(NEO, "neo.example", &[]));
    }
}
