//! Collection reads: allowlists, bounds, duplicates, and a total ordering.
//!
//! Every test here runs with **no storage of any kind**. FR-042 forbids a query, a connection, and
//! a persistence dependency in this phase; what is fixed here is the public shape Phase 006 will
//! connect to storage.

use renvor_validation::{CollectionContract, Direction, FilterOperator, PageBounds, Reason};

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn contract() -> CollectionContract {
    CollectionContract::new("id")
        .filterable("status", [FilterOperator::Eq, FilterOperator::Ne])
        .filterable("created_at", [FilterOperator::Ge, FilterOperator::Le])
        .sortable("created_at")
        .sortable("name")
        .includable("owner")
        .selectable("name")
        .selectable("status")
}

fn reasons(pairs: &[(String, String)]) -> Vec<Reason> {
    contract()
        .parse(pairs)
        .expect_err("expected refusal")
        .into_iter()
        .map(|issue| issue.reason)
        .collect()
}

#[test]
fn an_empty_query_yields_the_declared_default_page_size() {
    let query = contract().parse(&[]).expect("an empty query is valid");
    assert_eq!(query.page_size, PageBounds::default().default);
    assert!(query.cursor.is_none());
    assert!(query.filters.is_empty());
}

#[test]
fn the_ordering_is_total_even_when_the_caller_asked_for_nothing() {
    // The tiebreaker is what makes pages unable to skip or repeat a record. It is present when the
    // caller supplied no sort at all, because an optional correctness property is one that is
    // absent whenever nobody thought about it.
    let query = contract().parse(&[]).expect("valid");
    assert!(query.sort.terms.is_empty());
    assert_eq!(query.sort.tiebreaker, "id");

    let sorted = contract()
        .parse(&pairs(&[("sort", "-created_at")]))
        .expect("valid");
    assert_eq!(sorted.sort.terms.len(), 1);
    assert_eq!(sorted.sort.terms[0].field, "created_at");
    assert_eq!(sorted.sort.terms[0].direction, Direction::Descending);
    assert_eq!(
        sorted.sort.tiebreaker, "id",
        "the tiebreaker was dropped once the caller supplied a sort"
    );
}

#[test]
fn a_page_size_outside_the_bounds_is_refused_rather_than_clamped() {
    let bounds = PageBounds::default();

    // AT the boundaries: accepted.
    for size in [bounds.minimum, bounds.maximum] {
        let query = contract()
            .parse(&pairs(&[("page_size", &size.to_string())]))
            .unwrap_or_else(|issues| panic!("page_size={size} was refused: {issues:?}"));
        assert_eq!(query.page_size, size);
    }

    // BEYOND them: refused, and refused rather than silently corrected.
    for size in [bounds.minimum - 1, bounds.maximum + 1] {
        let issues = contract()
            .parse(&pairs(&[("page_size", &size.to_string())]))
            .expect_err("a page size outside the bounds must be refused");
        assert!(
            issues
                .iter()
                .any(|issue| issue.reason == Reason::OutOfRange),
            "page_size={size} produced {issues:?}"
        );
    }
}

#[test]
fn a_non_numeric_page_size_is_a_type_mismatch_not_a_range_failure() {
    // Different mistakes deserve different reasons: "not a number" and "out of range" call for
    // different corrections.
    assert!(reasons(&pairs(&[("page_size", "many")])).contains(&Reason::TypeMismatch));
}

#[test]
fn a_duplicate_key_is_refused_rather_than_resolved_by_position() {
    // Not last-one-wins. Not first-one-wins. A silent winner depends on ordering nobody wrote
    // down, which is the argument the route registry makes for duplicate routes.
    for key in ["page_size", "sort", "cursor", "include", "fields"] {
        let found = reasons(&pairs(&[(key, "1"), (key, "2")]));
        assert!(
            found.contains(&Reason::DuplicateKey),
            "a duplicate `{key}` was resolved silently; got {found:?}"
        );
    }
}

#[test]
fn an_unknown_sort_or_include_or_field_is_refused() {
    for (key, value) in [
        ("sort", "secret_column"),
        ("include", "everything"),
        ("fields", "password_hash"),
    ] {
        let found = reasons(&pairs(&[(key, value)]));
        assert!(
            found.contains(&Reason::NotAllowlisted),
            "`{key}={value}` was accepted; got {found:?}"
        );
    }

    // POSITIVE CONTROL: allowlisted values are accepted, so the refusals are about the allowlist
    // rather than about these parameters being rejected wholesale.
    contract()
        .parse(&pairs(&[
            ("sort", "name"),
            ("include", "owner"),
            ("fields", "name,status"),
        ]))
        .expect("allowlisted values must be accepted");
}

#[test]
fn an_unknown_key_is_never_echoed_back() {
    // The key is caller-chosen, so it must not appear in an issue. The pointer names the parameter
    // family instead.
    let canary = "CANARY-b41d97ce-UNKNOWN-KEY";
    let issues = contract()
        .parse(&pairs(&[("sort", canary)]))
        .expect_err("an unknown sort key must be refused");

    let rendered = format!("{issues:?}");
    assert!(
        !rendered.contains(canary),
        "the caller's key was echoed into an issue: {rendered}"
    );
    // POSITIVE CONTROL that the probe works.
    assert!(format!("{{\"k\":\"{canary}\"}}").contains(canary));
}

#[test]
fn a_filter_outside_the_allowlist_is_refused() {
    assert!(reasons(&pairs(&[("filter[password]", "x")])).contains(&Reason::NotAllowlisted));

    // POSITIVE CONTROL.
    let query = contract()
        .parse(&pairs(&[("filter[status]", "active")]))
        .expect("an allowlisted filter must be accepted");
    assert_eq!(query.filters.len(), 1);
    assert_eq!(query.filters[0].field, "status");
    assert_eq!(query.filters[0].operator, FilterOperator::Eq);
    assert_eq!(query.filters[0].value, "active");
}

#[test]
fn an_operator_outside_the_declared_set_for_that_field_is_refused() {
    // `status` declares only `eq` and `ne`. `ge` is a real operator, and not one for this field —
    // which is a different mistake from an operator that does not exist at all.
    assert!(reasons(&pairs(&[("filter[status][ge]", "x")])).contains(&Reason::OperatorNotAllowed));
    assert!(
        reasons(&pairs(&[("filter[status][like]", "x")])).contains(&Reason::OperatorNotAllowed)
    );

    // POSITIVE CONTROL: the operators the field DOES declare work.
    let query = contract()
        .parse(&pairs(&[("filter[created_at][ge]", "2026-01-01")]))
        .expect("a declared operator must be accepted");
    assert_eq!(query.filters[0].operator, FilterOperator::Ge);
}

#[test]
fn list_shaped_inputs_are_bounded() {
    let many_sorts = (0..10).map(|_| "name").collect::<Vec<_>>().join(",");
    assert!(reasons(&pairs(&[("sort", &many_sorts)])).contains(&Reason::TooManyTerms));

    let many_includes = (0..20).map(|_| "owner").collect::<Vec<_>>().join(",");
    assert!(reasons(&pairs(&[("include", &many_includes)])).contains(&Reason::TooManyTerms));
}

#[test]
fn an_invalid_cursor_is_refused_as_an_invalid_parameter() {
    assert!(reasons(&pairs(&[("cursor", "!!!not-a-cursor!!!")])).contains(&Reason::CursorInvalid));

    // POSITIVE CONTROL: a genuine cursor is accepted.
    let cursor = renvor_validation::Cursor::new(b"id:7".to_vec()).encode();
    let query = contract()
        .parse(&pairs(&[("cursor", &cursor)]))
        .expect("a genuine cursor must be accepted");
    assert_eq!(
        query.cursor.expect("a cursor was parsed").position(),
        b"id:7"
    );
}

#[test]
fn every_violation_is_reported_and_the_order_is_deterministic() {
    let query = pairs(&[
        ("page_size", "9999"),
        ("sort", "nope"),
        ("include", "nope"),
        ("cursor", "!!!"),
    ]);

    let first: Vec<String> = contract()
        .parse(&query)
        .expect_err("refusal")
        .into_iter()
        .map(|issue| format!("{}:{}", issue.pointer.as_str(), issue.reason.as_str()))
        .collect();

    assert!(first.len() >= 4, "validation stopped early: {first:?}");

    // Deterministic across runs.
    let second: Vec<String> = contract()
        .parse(&query)
        .expect_err("refusal")
        .into_iter()
        .map(|issue| format!("{}:{}", issue.pointer.as_str(), issue.reason.as_str()))
        .collect();
    assert_eq!(first, second);

    // And sorted, so it does not depend on the order the parameters happened to arrive in.
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted, "issue order is not deterministic by content");
}

#[test]
fn the_contract_denies_by_default() {
    // A contract that named nothing permits nothing. Constitution principle VI requires
    // deny-by-default, and a contract that started permissive would be one an author had to
    // remember to close.
    let empty = CollectionContract::new("id");
    for (key, value) in [
        ("sort", "anything"),
        ("include", "anything"),
        ("fields", "anything"),
        ("filter[anything]", "x"),
    ] {
        assert!(
            empty.parse(&pairs(&[(key, value)])).is_err(),
            "an empty contract accepted `{key}={value}`"
        );
    }
}
