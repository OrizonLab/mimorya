use super::*;

#[test]
fn each_formula_reads_back_as_stored_and_as_written_by_the_operator() {
    for formula in SupplyFormula::ALL {
        assert_eq!(SupplyFormula::parse(formula.as_str()), Some(formula));
    }
    assert_eq!(
        SupplyFormula::from_command("client-key"),
        Some(SupplyFormula::ClientKey)
    );
    assert_eq!(
        SupplyFormula::from_command(" prepaid "),
        Some(SupplyFormula::Prepaid)
    );
    assert_eq!(SupplyFormula::from_command("client_key"), None);
    assert_eq!(SupplyFormula::from_command("plafond"), None);
    assert!(!SupplyFormula::ClientKey.on_operator_key());
    assert!(SupplyFormula::Capped.on_operator_key() && SupplyFormula::Prepaid.on_operator_key());
    assert_eq!(SupplyFormula::default(), SupplyFormula::ClientKey);
}

#[test]
fn the_markup_raises_the_billed_amount_never_the_real_cost() {
    assert_eq!(billed_micros(200_000, 0), 200_000);
    assert_eq!(billed_micros(200_000, 1_500), 230_000);
    assert_eq!(billed_micros(200_000, MAX_MARKUP_BPS), 400_000);
    // Rounded half up to the micro-dollar.
    assert_eq!(billed_micros(3, 5_000), 5);
    assert_eq!(billed_micros(1, 4_999), 1);
    assert_eq!(billed_micros(0, 2_000), 0);
    // A markup past its bound counts as the bound.
    assert_eq!(billed_micros(100, 50_000), 200);
}

#[test]
fn caps_markups_and_credits_keep_to_their_bounds() {
    assert_eq!(cap_problem(DEFAULT_MONTHLY_CAP_MICROS), None);
    assert_eq!(cap_problem(MIN_MONTHLY_CAP_MICROS), None);
    assert_eq!(cap_problem(MAX_MONTHLY_CAP_MICROS), None);
    assert!(cap_problem(MIN_MONTHLY_CAP_MICROS - 1).is_some());
    assert!(cap_problem(MAX_MONTHLY_CAP_MICROS + 1).is_some());
    assert_eq!(markup_problem(MAX_MARKUP_BPS), None);
    assert!(markup_problem(MAX_MARKUP_BPS + 1).is_some());

    assert_eq!(credit_problem(500_000_000, "FAC-2026-0042"), None);
    assert_eq!(credit_problem(-20_000_000, "AV-2026-0003"), None);
    for (amount, reference) in [
        (0, "FAC-1"),
        (MAX_CREDIT_MICROS + 1, "FAC-1"),
        (-MAX_CREDIT_MICROS - 1, "FAC-1"),
        (1, ""),
        (1, "   "),
        (1, "FAC\n2"),
        (1, "FAC\u{200B}2"),
    ] {
        assert!(credit_problem(amount, reference).is_some(), "{reference:?}");
    }
    assert!(credit_problem(1, &"F".repeat(MAX_REFERENCE_CHARS + 1)).is_some());
    assert_eq!(credit_problem(1, &"F".repeat(MAX_REFERENCE_CHARS)), None);
}

#[test]
fn amounts_are_read_in_dollars_and_percentages_as_the_operator_writes_them() {
    assert_eq!(parse_usd("150", false), Ok(150_000_000));
    assert_eq!(parse_usd(" 150,5 ", false), Ok(150_500_000));
    assert_eq!(parse_usd("0.000001", false), Ok(1));
    assert_eq!(parse_usd("-20", true), Ok(-20_000_000));
    for refused in [
        "",
        "-20",
        "1.0000001",
        "abc",
        "1 000",
        "1e3",
        ".5",
        "12,",
        "1.2.3",
    ] {
        assert!(parse_usd(refused, false).is_err(), "{refused:?}");
    }
    assert!(parse_usd("--5", true).is_err());
    assert!(parse_usd("9999999999999", false).is_err());

    assert_eq!(parse_percent("15"), Ok(1_500));
    assert_eq!(parse_percent("12,5"), Ok(1_250));
    assert_eq!(parse_percent("0"), Ok(0));
    assert_eq!(parse_percent("7.25 %"), Ok(725));
    for refused in ["", "-5", "1.234", "quinze"] {
        assert!(parse_percent(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn amounts_are_shown_the_french_way() {
    assert_eq!(format_usd(150_000_000), "150,00 USD");
    assert_eq!(format_usd(1_234_500_000), "1\u{202F}234,50 USD");
    assert_eq!(format_usd(-20_000_000), "-20,00 USD");
    assert_eq!(format_usd(1), "0,000001 USD");
    assert_eq!(format_usd(0), "0,00 USD");
}
