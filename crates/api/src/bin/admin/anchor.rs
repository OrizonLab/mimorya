//! An anchor given to `mimorya-admin verify-audit` (ADR-178): a company's
//! place in its audit chain and the seal the sealing logged there, as
//! `<sigle>:<place>:<sceau>`, the seal in hexadecimal as logged.

/// A place of a company's chain and the seal logged there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// The company's slug.
    pub company: String,
    pub seq: i64,
    pub seal: Vec<u8>,
}

impl Anchor {
    /// The anchor `value` writes, or what is wrong with it.
    pub fn parse(value: &str) -> Result<Self, String> {
        let wrong =
            || format!("Ancre attendue sous la forme <sigle>:<place>:<sceau>, reçu : {value}");
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
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
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
        use mimorya_api::services::audit_chain::anchor_text;
        use mimorya_db::repositories::audit_chain::ChainHead;
        let company = uuid::Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_00ab);
        let head = ChainHead {
            chain_seq: 1234,
            chain_hash: (0..32).collect(),
        };
        let logged = anchor_text(company, &head);
        assert_eq!(
            Anchor::parse(&logged),
            Ok(Anchor {
                company: company.to_string(),
                seq: 1234,
                seal: (0..32).collect(),
            })
        );
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
        ] {
            assert!(Anchor::parse(&wrong).is_err(), "{wrong:?}");
        }
    }
}
