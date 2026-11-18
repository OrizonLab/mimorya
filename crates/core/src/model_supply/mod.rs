//! How a company's model is supplied (ADR-208): with its own key, or with
//! the operator's within a monthly cap, or against a prepaid credit. The
//! operator alone chooses the second and third formulas, sets the cap, the
//! markup and the credits; each call made on the operator's key carries
//! its re-invoiced amount beside its real cost. Pure rules: the
//! administration command and the platform apply them.

mod amounts;
mod limits;

pub use amounts::{format_usd, parse_percent, parse_usd};
pub use limits::{SupplyState, SupplyStop, supply_stop};

/// A monthly cap from 1 to 100,000 USD, 100 USD by default.
pub const MIN_MONTHLY_CAP_MICROS: i64 = 1_000_000;
pub const MAX_MONTHLY_CAP_MICROS: i64 = 100_000_000_000;
pub const DEFAULT_MONTHLY_CAP_MICROS: i64 = 100_000_000;
/// A markup from 0 to 100 %, in basis points.
pub const MAX_MARKUP_BPS: u32 = 10_000;
/// A credit entry of at most 1,000,000 USD, either way.
pub const MAX_CREDIT_MICROS: i64 = 1_000_000_000_000;
/// The reference of a credit entry (an invoice's number), in characters.
pub const MAX_REFERENCE_CHARS: usize = 100;

/// The formula of a company, or the one a call was made under.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SupplyFormula {
    /// Its own key, paid by the company to its provider.
    #[default]
    ClientKey,
    /// The operator's key, within a monthly cap.
    Capped,
    /// The operator's key, against a prepaid credit.
    Prepaid,
}

impl SupplyFormula {
    pub const ALL: [SupplyFormula; 3] = [Self::ClientKey, Self::Capped, Self::Prepaid];

    /// As stored (`tenant_model_supply.formula`, `usage_records.supply_formula`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClientKey => "client_key",
            Self::Capped => "capped",
            Self::Prepaid => "prepaid",
        }
    }

    pub fn parse(stored: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.as_str() == stored)
    }

    /// As the operator writes it on the command line.
    pub fn from_command(written: &str) -> Option<Self> {
        match written.trim() {
            "client-key" => Some(Self::ClientKey),
            "capped" => Some(Self::Capped),
            "prepaid" => Some(Self::Prepaid),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ClientKey => "clé du client",
            Self::Capped => "modèle fourni avec plafond",
            Self::Prepaid => "crédit prépayé",
        }
    }

    /// Whether the company may use the operator's key.
    pub fn on_operator_key(self) -> bool {
        !matches!(self, Self::ClientKey)
    }
}

/// The amount re-invoiced for a call that cost `cost_micros`, at `markup_bps`:
/// `cost × (1 + markup)`, rounded half up to the micro-dollar. The real cost
/// is never changed.
pub fn billed_micros(cost_micros: i64, markup_bps: u32) -> i64 {
    let scaled = i128::from(cost_micros) * (10_000 + i128::from(markup_bps.min(MAX_MARKUP_BPS)));
    let rounded = (scaled + 5_000).div_euclid(10_000);
    i64::try_from(rounded).unwrap_or(i64::MAX)
}

/// Why a monthly cap is refused, in French.
pub fn cap_problem(cap_micros: i64) -> Option<String> {
    (!(MIN_MONTHLY_CAP_MICROS..=MAX_MONTHLY_CAP_MICROS).contains(&cap_micros))
        .then(|| "Le plafond mensuel va de 1 à 100 000 USD".to_string())
}

/// Why a markup is refused, in French.
pub fn markup_problem(markup_bps: u32) -> Option<String> {
    (markup_bps > MAX_MARKUP_BPS).then(|| "Le coefficient va de 0 à 100 %".to_string())
}

/// Why a credit entry is refused, in French: a non-zero amount, a
/// correction being a negative one, and an invoice's reference on one
/// visible line.
pub fn credit_problem(amount_micros: i64, reference: &str) -> Option<String> {
    if amount_micros == 0 || amount_micros.abs() > MAX_CREDIT_MICROS {
        return Some(
            "Un versement va de 0,000001 à 1 000 000 USD, négatif pour une correction".into(),
        );
    }
    let reference = reference.trim();
    let visible = crate::plain_text::shows_all_it_holds(reference) && !reference.contains('\n');
    if reference.is_empty() || reference.chars().count() > MAX_REFERENCE_CHARS || !visible {
        return Some(format!(
            "La référence (numéro de facture) tient sur une ligne visible de 1 à \
             {MAX_REFERENCE_CHARS} caractères"
        ));
    }
    None
}

#[cfg(test)]
mod tests;
