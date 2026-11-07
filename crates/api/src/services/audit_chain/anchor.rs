//! An anchor (ADR-178): a company's place in its audit chain and the seal
//! the sealing logged there, kept outside the database, as
//! `<company>:<place>:<seal>` (`anchor_text`), the company by its
//! identifier as logged or by its slug, the seal in hexadecimal as logged.
//! Given to `mimorya-admin verify-audit` by the operator, or to the
//! verification an administrator asks for in their company (ADR-179).

use mimorya_db::repositories::audit_chain::ChainHead;
use uuid::Uuid;

/// A place of a company's chain and the seal logged there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// The company: its identifier, as logged, or its slug.
    pub company: String,
    pub seq: i64,
    pub seal: Vec<u8>,
}

impl Anchor {
    /// The anchor `value` writes, or what is wrong with it.
    pub fn parse(value: &str) -> Result<Self, String> {
        let wrong =
            || format!("Ancre attendue sous la forme <entreprise>:<place>:<sceau>, reçu : {value}");
        let mut parts = value.trim().split(':');
        let (Some(company), Some(seq), Some(seal), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(wrong());
        };
        let seq: i64 = seq.parse().map_err(|_| wrong())?;
        if company.is_empty() || seq < 1 {
            return Err(wrong());
        }
        let seal = unhex(seal)
            .filter(|seal| seal.len() == 32)
            .ok_or_else(|| format!("Sceau attendu en 64 chiffres hexadécimaux, reçu : {seal}"))?;
        Ok(Self {
            company: company.to_string(),
            seq,
            seal,
        })
    }

    /// Whether the anchor names the company `tenant_id`, by its identifier
    /// or by its slug `slug` (never an empty one: an anchor names a company).
    pub fn names(&self, tenant_id: Uuid, slug: &str) -> bool {
        self.company == slug || Uuid::parse_str(&self.company) == Ok(tenant_id)
    }

    /// The place and the seal the chain must hold there.
    pub fn head(&self) -> ChainHead {
        ChainHead {
            chain_seq: self.seq,
            chain_hash: self.seal.clone(),
        }
    }
}

/// The bytes `text` writes in hexadecimal digits, and nothing else: no sign
/// or space a number's reading would take.
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_logged_anchor() {
        let seal = "ab".repeat(32);
        assert_eq!(
            Anchor::parse(&format!("acme:42:{seal}")),
            Ok(Anchor {
                company: "acme".into(),
                seq: 42,
                seal: vec![0xab; 32],
            })
        );
        assert_eq!(
            Anchor::parse(&format!("acme:1:{}", "AB".repeat(32))).map(|anchor| anchor.seal),
            Ok(vec![0xab; 32])
        );
    }

    #[test]
    fn reads_an_anchor_as_the_sealing_logs_it() {
        use super::super::anchor_text;
        let company = uuid::Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_00ab);
        let head = ChainHead {
            chain_seq: 1234,
            chain_hash: (0..32).collect(),
        };
        let logged = anchor_text(company, &head);
        let anchor = Anchor::parse(&logged).unwrap();
        assert_eq!(
            anchor,
            Anchor {
                company: company.to_string(),
                seq: 1234,
                seal: (0..32).collect(),
            }
        );
        assert_eq!(anchor.head(), head);
    }

    #[test]
    fn names_a_company_by_its_identifier_or_its_slug() {
        let (acme, other) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let named = |company: &str| Anchor::parse(&format!("{company}:1:{}", "cd".repeat(32)));
        let by_id = named(&acme.to_string()).unwrap();
        assert!(by_id.names(acme, "acme") && by_id.names(acme, ""));
        assert!(!by_id.names(other, "acme"));
        let by_slug = named("acme").unwrap();
        assert!(by_slug.names(acme, "acme") && by_slug.names(other, "acme"));
        assert!(!by_slug.names(acme, "") && !by_slug.names(acme, "beta"));
        // An identifier written in capitals is the same identifier.
        let upper = named(&acme.to_string().to_uppercase()).unwrap();
        assert!(upper.names(acme, ""));
    }

    #[test]
    fn refuses_what_is_not_one() {
        let seal = "ab".repeat(32);
        for wrong in [
            String::new(),
            format!("acme:{seal}"),
            format!(":3:{seal}"),
            format!("acme:0:{seal}"),
            format!("acme:-2:{seal}"),
            format!("acme:trois:{seal}"),
            format!("acme:3:{seal}:4"),
            "acme:3:abcd".to_string(),
            format!("acme:3:{}", "zz".repeat(32)),
            format!("acme:3:{}é", "ab".repeat(31)),
            // What reading a number would take: a sign, a space.
            format!("acme:3:{}", "+a".repeat(32)),
            format!("acme:3:{} a", "ab".repeat(31)),
        ] {
            assert!(Anchor::parse(&wrong).is_err(), "{wrong:?}");
        }
    }
}
