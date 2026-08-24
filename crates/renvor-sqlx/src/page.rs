//! Binding a C-15 cursor to storage.
//!
//! # There is exactly one cursor format
//!
//! FR-037. [`renvor_database::Keyset`] already defines the opaque, fingerprinted, non-panicking
//! cursor that contract C-15 describes. This module does **not** define a second one for the
//! database's benefit — it turns the ordering that cursor was issued under into a SQL predicate,
//! and nothing else. A storage-specific cursor format would mean two encodings to keep in step,
//! and the one that drifted would be the one nobody tested.
//!
//! # Why keyset and not `OFFSET`
//!
//! `OFFSET n` makes the database count and discard `n` rows on every page, so the last page of a
//! large collection is the most expensive one to serve — and a row inserted or deleted during
//! paging shifts every later page, silently skipping or repeating rows. A keyset seek asks for
//! "rows after this position" and is unaffected by both.

use renvor_database::{DatabaseError, DatabaseErrorKind, DatabaseKind, Direction, OrderBy};

/// Renders the `WHERE` predicate that resumes an ordering after a position.
///
/// Produces a **row-value comparison** — `(a, b) > ($1, $2)` — which both PostgreSQL and MySQL
/// evaluate lexicographically. Writing it as nested `OR` groups would be equivalent in principle
/// and much easier to get subtly wrong; row-value syntax is one construct that both engines
/// implement identically, which is what FR-041 needs.
///
/// `first_placeholder` is the one-based index of the first bound parameter, so a caller that has
/// already bound filter values can continue the numbering rather than restarting it.
///
/// # Every identifier here is a compile-time constant
///
/// The columns come from [`OrderBy`], whose terms are `&'static str` drawn from a
/// [`renvor_database::SortAllowlist`]. There is no caller text in the output, and the values the
/// predicate compares against are **bound**, never rendered.
///
/// # Errors
///
/// [`DatabaseErrorKind::StatementRejected`] when the ordering mixes directions. A mixed ordering
/// cannot be expressed as a single row-value comparison, and the nested-`OR` form that would be
/// required is deliberately not generated here — see the module note in
/// `governance/phase-006-evidence.md`. Refused rather than silently served with the wrong
/// semantics.
pub fn seek_predicate(
    order: &OrderBy,
    kind: DatabaseKind,
    first_placeholder: usize,
) -> Result<String, DatabaseError> {
    let terms = order.terms();
    let leading = terms
        .first()
        .ok_or_else(|| DatabaseError::new(DatabaseErrorKind::StatementRejected))?
        .direction();

    if terms.iter().any(|term| term.direction() != leading) {
        return Err(DatabaseError::new(DatabaseErrorKind::StatementRejected));
    }

    let columns = terms
        .iter()
        .map(|term| kind.quote_identifier(term.column()))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (0..terms.len())
        .map(|offset| kind.placeholder(first_placeholder + offset))
        .collect::<Vec<_>>()
        .join(", ");
    let comparison = match leading {
        Direction::Ascending => ">",
        Direction::Descending => "<",
    };

    Ok(format!("({columns}) {comparison} ({placeholders})"))
}

/// Renders `ORDER BY` for this database's quoting rules.
///
/// A thin wrapper, but it keeps every caller from having to remember which quoting function goes
/// with which database — which is the kind of thing that is right everywhere until it is wrong in
/// one place.
#[must_use]
pub fn order_clause(order: &OrderBy, kind: DatabaseKind) -> String {
    order.render(|column| kind.quote_identifier(column))
}

/// The `LIMIT` both databases accept.
///
/// Rendered from a `usize` Renvor owns, never from caller text. PostgreSQL and MySQL agree on
/// `LIMIT n`, so no dialect branch is needed — and one is not invented on the chance that a future
/// database disagrees.
#[must_use]
pub fn limit_clause(rows: usize) -> String {
    format!("LIMIT {rows}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use renvor_database::SortAllowlist;

    fn ascending() -> OrderBy {
        SortAllowlist::new()
            .allow("name", "name")
            .tiebreaker("id")
            .order_by(&[("name".to_owned(), Direction::Ascending)])
            .expect("allowed")
    }

    #[test]
    fn postgres_numbers_its_placeholders_and_mysql_does_not() {
        let order = ascending();
        assert_eq!(
            seek_predicate(&order, DatabaseKind::Postgres, 1).expect("renders"),
            r#"("name", "id") > ($1, $2)"#
        );
        assert_eq!(
            seek_predicate(&order, DatabaseKind::MySql, 1).expect("renders"),
            "(`name`, `id`) > (?, ?)"
        );
    }

    #[test]
    fn placeholder_numbering_continues_from_the_filters() {
        let order = ascending();
        assert_eq!(
            seek_predicate(&order, DatabaseKind::Postgres, 4).expect("renders"),
            r#"("name", "id") > ($4, $5)"#
        );
    }

    #[test]
    fn a_descending_ordering_seeks_backwards() {
        let order = SortAllowlist::new()
            .allow("name", "name")
            .tiebreaker("id")
            .order_by(&[("name".to_owned(), Direction::Descending)])
            .expect("allowed");
        let rendered = seek_predicate(&order, DatabaseKind::Postgres, 1).expect("renders");
        assert!(
            rendered.contains('<'),
            "descending must seek backwards: {rendered}"
        );
    }

    /// A mixed ordering is REFUSED, not served with the wrong semantics.
    #[test]
    fn a_mixed_ordering_is_refused() {
        let order = SortAllowlist::new()
            .allow("name", "name")
            .allow("rank", "rank_value")
            .tiebreaker("id")
            .order_by(&[
                ("name".to_owned(), Direction::Ascending),
                ("rank".to_owned(), Direction::Descending),
            ])
            .expect("allowed");
        let error = seek_predicate(&order, DatabaseKind::Postgres, 1).expect_err("refuses");
        assert_eq!(error.kind(), DatabaseErrorKind::StatementRejected);
    }

    /// No caller text reaches the rendered predicate.
    #[test]
    fn the_predicate_contains_only_allowlisted_identifiers() {
        let order = ascending();
        for kind in DatabaseKind::ALL {
            let rendered = seek_predicate(&order, kind, 1).expect("renders");
            assert!(!rendered.contains("DROP"));
            assert!(!rendered.contains(';'));
        }
    }
}
