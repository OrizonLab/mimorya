//! Amounts as the operator writes them on the command line, and as the
//! command shows them: US dollars with at most six decimals (a micro-dollar),
//! a point or a comma between their units and decimals; a percentage with at
//! most two (a basis point).

/// The micro-dollars `text` writes, a minus sign allowed when `signed`;
/// why not, in French.
pub fn parse_usd(text: &str, signed: bool) -> Result<i64, String> {
    let refused = || {
        format!(
            "Montant invalide : « {} » (en USD, par exemple 150 ou 150,50)",
            text.trim()
        )
    };
    let written = text.trim();
    let (negative, digits) = match written.strip_prefix('-') {
        Some(rest) if signed => (true, rest),
        _ => (false, written),
    };
    let micros = decimal(digits, 6).ok_or_else(refused)?;
    Ok(if negative { -micros } else { micros })
}

/// The basis points `text` writes as a percentage (`15`, `12,5`); why not,
/// in French.
pub fn parse_percent(text: &str) -> Result<u32, String> {
    let written = text.trim().trim_end_matches('%').trim_end();
    decimal(written, 2)
        .and_then(|bps| u32::try_from(bps).ok())
        .ok_or_else(|| {
            format!(
                "Pourcentage invalide : « {} » (par exemple 15 ou 12,5)",
                text.trim()
            )
        })
}

/// `micros` as dollars with two decimals, the French way (`1 234,50 USD`);
/// a sub-cent amount keeps its decimals.
pub fn format_usd(micros: i64) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let micros = micros.unsigned_abs();
    let (dollars, fraction) = (micros / 1_000_000, micros % 1_000_000);
    let mut decimals = format!("{fraction:06}");
    while decimals.len() > 2 && decimals.ends_with('0') {
        decimals.pop();
    }
    format!("{sign}{},{decimals} USD", grouped(dollars))
}

/// `digits` in thousands parted by a narrow space.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push('\u{202F}');
        }
        out.push(digit);
    }
    out
}

/// The units of `written` (digits, then at most `scale` decimals after a
/// point or a comma), in its smallest unit; `None` when it writes anything
/// else or too much.
fn decimal(written: &str, scale: u32) -> Option<i64> {
    let (units, decimals) = match written.split_once(['.', ',']) {
        // A separator is followed by decimals.
        Some((_, "")) => return None,
        Some((units, decimals)) => (units, decimals),
        None => (written, ""),
    };
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if units.is_empty() || !digits(units) || !digits(decimals) {
        return None;
    }
    if decimals.len() > scale as usize || units.len() > 12 {
        return None;
    }
    let units: i64 = units.parse().ok()?;
    let padded = format!("{decimals:0<width$}", width = scale as usize);
    let fraction: i64 = if padded.is_empty() {
        0
    } else {
        padded.parse().ok()?
    };
    units.checked_mul(10_i64.pow(scale))?.checked_add(fraction)
}
