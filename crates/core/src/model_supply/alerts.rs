//! When the operator's supply of a company is worth an alert to its
//! administrators (ADR-211): its month coming to a share of its monthly
//! cap, or its prepaid credit's balance falling to a share of its last
//! payment. The operator sets the shares, within bounds; each alert is sent
//! once for its month or its payment.

/// By default: 80 and 95 % of the cap.
pub const DEFAULT_CAP_ALERTS: [u8; 2] = [80, 95];
/// By default: 20 then 5 % of the last payment, kept the lowest first.
pub const DEFAULT_CREDIT_ALERTS: [u8; 2] = [5, 20];
/// At most this many shares of each kind.
pub const MAX_ALERTS: usize = 5;

/// What an alert is read against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// A share of the monthly cap, from 1 to 100 % (the cap reached).
    Cap,
    /// A share of the last payment, from 0 (the credit spent) to 99 %.
    Credit,
}

impl AlertKind {
    /// As stored (`tenant_supply_alerts.kind`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cap => "cap",
            Self::Credit => "credit",
        }
    }

    fn bounds(self) -> (u8, u8) {
        match self {
            Self::Cap => (1, 100),
            Self::Credit => (0, 99),
        }
    }
}

/// Why `percents` may not be the shares of `kind`, in French.
pub fn alerts_problem(kind: AlertKind, percents: &[u8]) -> Option<String> {
    if percents.len() > MAX_ALERTS {
        return Some(format!("Au plus {MAX_ALERTS} seuils d’alerte"));
    }
    let (low, high) = kind.bounds();
    if let Some(out) = percents.iter().find(|p| !(low..=high).contains(*p)) {
        return Some(format!(
            "Seuil d’alerte hors bornes : {out} % (de {low} à {high} %)"
        ));
    }
    let mut seen = percents.to_vec();
    seen.sort_unstable();
    seen.dedup();
    (seen.len() != percents.len()).then(|| "Un seuil d’alerte est donné deux fois".into())
}

/// The shares an operator writes (`80,95`), or none (`aucun`), checked for
/// `kind`; sorted, the lowest first.
pub fn parse_alerts(kind: AlertKind, written: &str) -> Result<Vec<u8>, String> {
    let written = written.trim();
    if written.eq_ignore_ascii_case("aucun") || written.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let mut percents = Vec::new();
    for part in written.split(',') {
        let part = part.trim().trim_end_matches('%').trim();
        let percent = part
            .parse::<u8>()
            .map_err(|_| format!("Seuil d’alerte illisible : « {part} » (un entier, en %)"))?;
        percents.push(percent);
    }
    if let Some(problem) = alerts_problem(kind, &percents) {
        return Err(problem);
    }
    percents.sort_unstable();
    Ok(percents)
}

/// The shares of the cap the month came to: what it was re-invoiced is at
/// least that share of `cap_micros`.
pub fn cap_alerts_reached(month_billed_micros: i64, cap_micros: i64, percents: &[u8]) -> Vec<u8> {
    let billed = i128::from(month_billed_micros) * 100;
    percents
        .iter()
        .copied()
        .filter(|p| billed >= i128::from(cap_micros) * i128::from(*p))
        .collect()
}

/// The shares of the last payment the credit's balance fell to: it is at
/// most that share of `last_payment_micros`.
pub fn credit_alerts_reached(
    balance_micros: i64,
    last_payment_micros: i64,
    percents: &[u8],
) -> Vec<u8> {
    let balance = i128::from(balance_micros) * 100;
    percents
        .iter()
        .copied()
        .filter(|p| balance <= i128::from(last_payment_micros) * i128::from(*p))
        .collect()
}

/// Of the shares `reached`, the one an alert tells: the highest of the cap,
/// the lowest of the payment.
pub fn most_pressing(kind: AlertKind, reached: &[u8]) -> Option<u8> {
    match kind {
        AlertKind::Cap => reached.iter().copied().max(),
        AlertKind::Credit => reached.iter().copied().min(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shares_are_read_and_kept_within_their_bounds() {
        assert_eq!(parse_alerts(AlertKind::Cap, "95, 80"), Ok(vec![80, 95]));
        assert_eq!(parse_alerts(AlertKind::Cap, "100%"), Ok(vec![100]));
        assert_eq!(parse_alerts(AlertKind::Credit, "aucun"), Ok(Vec::new()));
        assert_eq!(
            parse_alerts(AlertKind::Credit, "0,5,20"),
            Ok(vec![0, 5, 20])
        );
        for (kind, written) in [
            (AlertKind::Cap, "0"),
            (AlertKind::Cap, "101"),
            (AlertKind::Credit, "100"),
            (AlertKind::Cap, "80,80"),
            (AlertKind::Cap, "10,20,30,40,50,60"),
            (AlertKind::Cap, "quatre-vingts"),
            (AlertKind::Cap, "-5"),
        ] {
            assert!(parse_alerts(kind, written).is_err(), "{written}");
        }
        assert_eq!(alerts_problem(AlertKind::Cap, &DEFAULT_CAP_ALERTS), None);
        assert_eq!(
            alerts_problem(AlertKind::Credit, &DEFAULT_CREDIT_ALERTS),
            None
        );
    }

    #[test]
    fn a_share_of_the_cap_is_reached_once_the_month_comes_to_it() {
        let cap = 100_000_000;
        assert!(cap_alerts_reached(79_999_999, cap, &[80, 95]).is_empty());
        assert_eq!(cap_alerts_reached(80_000_000, cap, &[80, 95]), [80]);
        assert_eq!(cap_alerts_reached(99_000_000, cap, &[80, 95]), [80, 95]);
        assert_eq!(cap_alerts_reached(i64::MAX, i64::MAX, &[100]), [100]);
        assert_eq!(most_pressing(AlertKind::Cap, &[80, 95]), Some(95));
    }

    #[test]
    fn a_share_of_the_payment_is_reached_once_the_balance_falls_to_it() {
        let paid = 50_000_000;
        assert!(credit_alerts_reached(10_000_001, paid, &[20, 5]).is_empty());
        assert_eq!(credit_alerts_reached(10_000_000, paid, &[20, 5]), [20]);
        assert_eq!(credit_alerts_reached(-1, paid, &[20, 5, 0]), [20, 5, 0]);
        assert!(credit_alerts_reached(1, paid, &[0]).is_empty());
        assert_eq!(most_pressing(AlertKind::Credit, &[20, 5]), Some(5));
        assert_eq!(most_pressing(AlertKind::Credit, &[]), None);
    }
}
