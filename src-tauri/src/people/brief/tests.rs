use super::*;
use serde_json::json;

#[test]
fn the_cadence_table_matches_the_one_that_reminds() {
    // Two tables would mean the dot beside somebody's name turning red on a
    // different day from the one the notification arrives.
    for (frequency, days) in [
        ("weekly", 7),
        ("biweekly", 14),
        ("monthly", 30),
        ("quarterly", 90),
        ("yearly", 365),
    ] {
        assert_eq!(cadence_days(frequency), Some(days), "{frequency}");
    }
    assert_eq!(cadence_days("fortnightly"), None);
    assert_eq!(cadence_days(""), None);
}

#[test]
fn the_status_bands_match_the_coloured_dot() {
    // Boundaries as fractions of a 30-day cadence: .5, .85, 1.2
    assert_eq!(contact_status(Some(0), Some(30)), "thriving");
    assert_eq!(contact_status(Some(15), Some(30)), "thriving");
    assert_eq!(contact_status(Some(16), Some(30)), "on_track");
    assert_eq!(contact_status(Some(25), Some(30)), "on_track");
    assert_eq!(contact_status(Some(26), Some(30)), "due_soon");
    assert_eq!(contact_status(Some(36), Some(30)), "due_soon");
    assert_eq!(contact_status(Some(37), Some(30)), "overdue");
}

#[test]
fn nothing_to_count_from_is_not_a_status() {
    assert_eq!(contact_status(None, Some(30)), "unknown");
    assert_eq!(contact_status(Some(40), None), "unknown");
    assert_eq!(contact_status(Some(40), Some(0)), "unknown");
}

#[test]
fn relationships_are_read_in_either_shape() {
    assert_eq!(
        relationships_of(&json!({ "relationship_type": ["Friend", "Colleague"] })),
        ["Friend", "Colleague"]
    );
    // A vault written before relationships became a list.
    assert_eq!(
        relationships_of(&json!({ "relationship_type": "Friend, Colleague" })),
        ["Friend", "Colleague"]
    );
    assert!(relationships_of(&json!({})).is_empty());
    assert!(relationships_of(&json!({ "relationship_type": "" })).is_empty());
}

// ─── Reciprocity ────────────────────────────────────────────

#[test]
fn somebody_with_no_history_is_not_reported_as_balanced() {
    // Zero and "nothing recorded" are different answers, and showing an
    // even balance for a person nothing is known about would be a lie.
    let got = reciprocity(&json!({}), &[], &[]);
    assert!(!got.has_history);
    assert_eq!(got, Reciprocity::default());
}

#[test]
fn gifts_are_counted_in_the_direction_they_went() {
    let got = reciprocity(
        &json!({ "gifts": [
            { "name": "Book", "direction": "given" },
            { "name": "Wine", "direction": "received" },
            // No direction recorded is a gift you gave: that is what the form
            // writes by default, and what most entries are.
            { "name": "Flowers" },
        ]}),
        &[],
        &[],
    );
    assert_eq!(got.gifts_given, 2);
    assert_eq!(got.gifts_received, 1);
    assert!(got.has_history);
}

#[test]
fn money_is_counted_in_the_direction_it_moved() {
    let got = reciprocity(
        &json!({}),
        &[
            json!({ "type": "expense", "amount": 250_000.0 }),
            json!({ "type": "transfer", "amount": 500_000.0 }),
            json!({ "type": "income", "amount": 100_000.0 }),
        ],
        &[],
    );
    // An expense tagged to somebody is money spent on them; a transfer is
    // money sent their way. Both went out.
    assert_eq!(got.money_out, 750_000.0);
    assert_eq!(got.money_in, 100_000.0);
}

#[test]
fn a_transaction_of_nothing_is_not_a_history() {
    let got = reciprocity(&json!({}), &[json!({ "type": "expense", "amount": 0.0 })], &[]);
    assert!(!got.has_history);
}

#[test]
fn what_is_still_owed_is_signed_towards_whoever_is_owed_it() {
    // Positive: they owe you.
    let lent = reciprocity(
        &json!({}),
        &[],
        &[json!({ "type": "lend", "totalAmount": 5_000_000.0, "paidAmount": 2_000_000.0 })],
    );
    assert_eq!(lent.outstanding, 3_000_000.0);

    // Negative: you owe them.
    let borrowed = reciprocity(
        &json!({}),
        &[],
        &[json!({ "type": "borrow", "totalAmount": 1_000_000.0, "paidAmount": 0.0 })],
    );
    assert_eq!(borrowed.outstanding, -1_000_000.0);
}

#[test]
fn a_debt_paid_off_leaves_nothing_outstanding() {
    let got = reciprocity(
        &json!({}),
        &[],
        &[json!({ "type": "lend", "totalAmount": 5_000_000.0, "paidAmount": 5_000_000.0 })],
    );
    assert_eq!(got.outstanding, 0.0);
    // But it is still history: something did pass between you.
    assert!(got.has_history);
}

#[test]
fn overpaying_a_debt_does_not_turn_it_around() {
    // A rounding error or a generous repayment should not read as the other
    // person now owing money in the opposite direction.
    let got = reciprocity(
        &json!({}),
        &[],
        &[json!({ "type": "lend", "totalAmount": 1_000_000.0, "paidAmount": 1_200_000.0 })],
    );
    assert_eq!(got.outstanding, 0.0);
}

#[test]
fn everything_together_reads_as_one_picture() {
    let got = reciprocity(
        &json!({ "gifts": [{ "direction": "given" }, { "direction": "received" }] }),
        &[
            json!({ "type": "expense", "amount": 250_000.0 }),
            json!({ "type": "income", "amount": 80_000.0 }),
        ],
        &[json!({ "type": "lend", "totalAmount": 2_000_000.0, "paidAmount": 500_000.0 })],
    );
    assert_eq!(
        got,
        Reciprocity {
            gifts_given: 1,
            gifts_received: 1,
            money_out: 250_000.0,
            money_in: 80_000.0,
            outstanding: 1_500_000.0,
            has_history: true,
        }
    );
}
