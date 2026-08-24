//! Keyset pagination — contract C-15 bound to storage.
//!
//! # This module introduces no second cursor format
//!
//! [`renvor_validation::Cursor`] already fixes the encoding, the version byte, the size bound, and
//! the non-panicking decode. `contracts/collection-reads.md` states that *"Phase 006 connects that
//! shape to storage"*, and connecting is what this module does — it gives the opaque position
//! bytes a **meaning**, and nothing else.
//!
//! # Identifier injection is made unrepresentable rather than filtered
//!
//! A dynamic `ORDER BY` is the one place a collection read needs an identifier that varies at
//! runtime, and an identifier cannot be a bound parameter. The usual defence is to validate the
//! caller's text and then interpolate it, which works until the validator has a gap.
//!
//! [`SortAllowlist`] does something stronger. It maps a caller-visible **field name** to a
//! `&'static str` **column identifier**, so the text that reaches SQL is a compile-time constant
//! chosen by the application author. No caller-supplied byte can inhabit `&'static str`, which is
//! the same construction `renvor_error::ApiErrorCode::detail` uses for redaction.

use std::collections::BTreeMap;

use renvor_validation::collection::Direction;
use renvor_validation::{Cursor, CursorError};

use crate::error::{DatabaseError, DatabaseErrorKind};

/// The largest number of sort terms an ordering may carry.
///
/// Every list-shaped input carries a maximum, asserted at the boundary and boundary + 1 —
/// `contracts/collection-reads.md`, and this is a list-shaped input.
pub const MAX_SORT_TERMS: usize = 8;

/// A closed map from caller-visible field names to physical column identifiers.
///
/// # The value type is `&'static str`, and that is the whole security property
///
/// See the module documentation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SortAllowlist {
    columns: BTreeMap<String, &'static str>,
    tiebreaker: Option<&'static str>,
}

impl SortAllowlist {
    /// An empty allowlist. Nothing is sortable until it is named.
    ///
    /// Deny-by-default, matching C-15: *"A field is filterable, sortable, includable, or selectable
    /// only once it is named."*
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Permits `field`, ordering by `column`.
    ///
    /// `column` is `&'static str` deliberately. See the module documentation.
    #[must_use]
    pub fn allow(mut self, field: impl Into<String>, column: &'static str) -> Self {
        self.columns.insert(field.into(), column);
        self
    }

    /// Declares the tiebreaker column that makes every ordering total.
    ///
    /// # A tiebreaker is not optional
    ///
    /// C-15: *"With a non-total ordering, two tied rows can be returned in different relative
    /// orders across pages, so a record is skipped or repeated — and nothing in the response says
    /// so."* [`SortAllowlist::order_by`] refuses to build an ordering without one, so the property
    /// cannot be lost by forgetting rather than by deciding.
    #[must_use]
    pub const fn tiebreaker(mut self, column: &'static str) -> Self {
        self.tiebreaker = Some(column);
        self
    }

    /// The column for `field`, if it is allowed.
    #[must_use]
    pub fn column(&self, field: &str) -> Option<&'static str> {
        self.columns.get(field).copied()
    }

    /// Whether `field` is sortable.
    #[must_use]
    pub fn is_allowed(&self, field: &str) -> bool {
        self.columns.contains_key(field)
    }

    /// Resolves validated sort terms into a total ordering.
    ///
    /// # Errors
    ///
    /// - [`DatabaseErrorKind::Unclassified`] when no tiebreaker was declared, when a field is not
    ///   on the allowlist, or when there are more than [`MAX_SORT_TERMS`] terms.
    ///
    /// A refused field is **not** echoed. C-15: *"A key that failed it is caller-chosen, so the
    /// issue names the parameter family."* The error carries the family and not the text, which is
    /// automatic here because [`DatabaseError`] has no field text can inhabit.
    pub fn order_by(&self, terms: &[(String, Direction)]) -> Result<OrderBy, DatabaseError> {
        let Some(tiebreaker) = self.tiebreaker else {
            return Err(DatabaseError::new(DatabaseErrorKind::Unclassified));
        };
        if terms.len() > MAX_SORT_TERMS {
            return Err(DatabaseError::new(DatabaseErrorKind::Unclassified));
        }

        let mut resolved = Vec::with_capacity(terms.len() + 1);
        for (field, direction) in terms {
            let column = self
                .column(field)
                .ok_or_else(|| DatabaseError::new(DatabaseErrorKind::Unclassified))?;
            resolved.push(OrderTerm {
                column,
                direction: *direction,
            });
        }

        // THE TIEBREAKER IS ALWAYS APPENDED, including when the caller declared no sort at all.
        // Appending it unconditionally is what makes totality a property of the type rather than
        // of the caller remembering.
        //
        // IT INHERITS THE PRECEDING TERM'S DIRECTION, and that is not a cosmetic choice. A
        // tiebreaker exists to make an ordering total, not to introduce a second scan direction.
        // Pinning it to `Ascending` made `name DESC` resolve to `name DESC, id ASC`, which is
        // valid SQL and a perfectly deterministic order — but it cannot be expressed as a single
        // row-value comparison, so every descending sort became unpageable by keyset seek. That
        // was found by `renvor-sqlx`'s `seek_predicate`, which refuses a mixed ordering rather
        // than emitting one whose semantics it cannot honour.
        if !resolved.iter().any(|term| term.column == tiebreaker) {
            let direction = resolved
                .last()
                .map_or(Direction::Ascending, |term| term.direction);
            resolved.push(OrderTerm {
                column: tiebreaker,
                direction,
            });
        }

        Ok(OrderBy { terms: resolved })
    }
}

/// One resolved ordering term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderTerm {
    column: &'static str,
    direction: Direction,
}

impl OrderTerm {
    /// The physical column. A compile-time constant, never caller text.
    #[must_use]
    pub const fn column(self) -> &'static str {
        self.column
    }

    /// The direction.
    #[must_use]
    pub const fn direction(self) -> Direction {
        self.direction
    }

    /// The SQL keyword for this direction.
    ///
    /// A closed set of two constants. There is no third value and no caller input.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self.direction {
            Direction::Ascending => "ASC",
            Direction::Descending => "DESC",
        }
    }
}

/// A total ordering, every term drawn from an allowlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderBy {
    terms: Vec<OrderTerm>,
}

impl OrderBy {
    /// The terms, in order. Never empty: a tiebreaker is always present.
    #[must_use]
    pub fn terms(&self) -> &[OrderTerm] {
        &self.terms
    }

    /// Renders the `ORDER BY` list, with `quote` applied to each identifier.
    ///
    /// The quoting function differs per database — `"x"` for PostgreSQL, `` `x` `` for MySQL — so
    /// it is supplied by the adapter rather than guessed here. Every value passed to it is a
    /// `&'static str` from the allowlist.
    #[must_use]
    pub fn render(&self, quote: impl Fn(&str) -> String) -> String {
        self.terms
            .iter()
            .map(|term| format!("{} {}", quote(term.column()), term.keyword()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A resumable position, plus the fingerprint of the query it was issued for.
///
/// # Why the fingerprint travels with the position
///
/// A cursor names a place in *an ordering over a filtered set*. Presenting it against a different
/// filter or a different sort yields a position that is syntactically fine and semantically
/// meaningless — the exact hazard C-15 describes for a foreign cursor. Carrying the fingerprint
/// lets that be **refused** rather than served.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyset {
    fingerprint: u64,
    values: Vec<Vec<u8>>,
}

impl Keyset {
    /// Builds a keyset position for a query fingerprint.
    #[must_use]
    pub fn new(fingerprint: u64, values: Vec<Vec<u8>>) -> Self {
        Self {
            fingerprint,
            values,
        }
    }

    /// The fingerprint of the query this position belongs to.
    #[must_use]
    pub const fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// The ordered values that locate the last row of the previous page.
    #[must_use]
    pub fn values(&self) -> &[Vec<u8>] {
        &self.values
    }

    /// Encodes this position into a C-15 cursor.
    #[must_use]
    pub fn to_cursor(&self) -> Cursor {
        let mut bytes = Vec::with_capacity(16 + self.values.len() * 8);
        bytes.extend_from_slice(&self.fingerprint.to_be_bytes());
        // A u16 count bounds the term list at decode time without a separate length check.
        let count = u16::try_from(self.values.len()).unwrap_or(u16::MAX);
        bytes.extend_from_slice(&count.to_be_bytes());
        for value in self.values.iter().take(count as usize) {
            let len = u16::try_from(value.len()).unwrap_or(u16::MAX);
            bytes.extend_from_slice(&len.to_be_bytes());
            bytes.extend_from_slice(&value[..len as usize]);
        }
        Cursor::new(bytes)
    }

    /// Decodes a cursor into a position, refusing one issued for a different query.
    ///
    /// # This function does not panic, for any input
    ///
    /// Every read is length-checked before it is taken. The cursor's own decoder already refuses
    /// an over-long or wrongly-versioned value before these bytes exist.
    ///
    /// # Errors
    ///
    /// [`KeysetError::Cursor`] when the cursor itself is invalid, [`KeysetError::Malformed`] when
    /// the payload is truncated, and [`KeysetError::ForeignQuery`] when the fingerprint does not
    /// match — which is the "changed filters" refusal C-15 requires.
    pub fn from_cursor(text: &str, fingerprint: u64) -> Result<Self, KeysetError> {
        let cursor = Cursor::decode(text).map_err(KeysetError::Cursor)?;
        let bytes = cursor.position();

        let head = bytes.get(..10).ok_or(KeysetError::Malformed)?;
        let mut fp = [0_u8; 8];
        fp.copy_from_slice(&head[..8]);
        let declared = u64::from_be_bytes(fp);

        // THE FINGERPRINT IS CHECKED BEFORE THE PAYLOAD IS INTERPRETED, for the same reason C-15
        // checks the version first: a best-effort read of a foreign position yields a page from
        // nowhere, and the caller cannot tell.
        if declared != fingerprint {
            return Err(KeysetError::ForeignQuery);
        }

        let count = u16::from_be_bytes([head[8], head[9]]) as usize;
        if count > MAX_SORT_TERMS + 1 {
            return Err(KeysetError::Malformed);
        }

        let mut values = Vec::with_capacity(count);
        let mut offset = 10_usize;
        for _ in 0..count {
            let len_bytes = bytes
                .get(offset..offset + 2)
                .ok_or(KeysetError::Malformed)?;
            let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;
            offset += 2;
            let value = bytes
                .get(offset..offset + len)
                .ok_or(KeysetError::Malformed)?;
            offset += len;
            values.push(value.to_vec());
        }

        Ok(Self {
            fingerprint,
            values,
        })
    }
}

/// Why a keyset could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeysetError {
    /// The cursor itself was refused by C-15's decoder.
    Cursor(CursorError),
    /// The payload was truncated or over-long.
    Malformed,
    /// The cursor was issued for a different filter or sort.
    ForeignQuery,
}

impl core::fmt::Display for KeysetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Cursor(inner) => write!(f, "{inner}"),
            Self::Malformed => f.write_str("the cursor payload is malformed"),
            Self::ForeignQuery => {
                f.write_str("the cursor was issued for a different query and cannot be resumed")
            }
        }
    }
}

impl core::error::Error for KeysetError {}

/// One page of results, plus the position that resumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<T> {
    items: Vec<T>,
    next: Option<Cursor>,
}

impl<T> Page<T> {
    /// Builds a page.
    #[must_use]
    pub const fn new(items: Vec<T>, next: Option<Cursor>) -> Self {
        Self { items, next }
    }

    /// The items.
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// The cursor that resumes after this page, if more may follow.
    #[must_use]
    pub const fn next(&self) -> Option<&Cursor> {
        self.next.as_ref()
    }

    /// Consumes the page, yielding its items.
    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist() -> SortAllowlist {
        SortAllowlist::new()
            .allow("title", "title")
            .allow("published_at", "published_at")
            .tiebreaker("id")
    }

    #[test]
    fn an_ordering_is_total_even_when_no_sort_was_declared() {
        let order = allowlist().order_by(&[]).expect("builds");
        assert_eq!(order.terms().len(), 1);
        assert_eq!(order.terms()[0].column(), "id");
    }

    #[test]
    fn the_tiebreaker_is_appended_to_a_declared_sort() {
        let order = allowlist()
            .order_by(&[("title".to_owned(), Direction::Descending)])
            .expect("builds");
        assert_eq!(order.terms().len(), 2);
        assert_eq!(order.terms()[0].column(), "title");
        assert_eq!(order.terms()[0].keyword(), "DESC");
        assert_eq!(order.terms()[1].column(), "id");
        // INHERITED, not pinned to `ASC`. See `order_by` for why: a tiebreaker running counter to
        // the primary sort makes the ordering unpageable by a keyset seek.
        assert_eq!(order.terms()[1].keyword(), "DESC");
    }

    /// Every term in a resolved ordering shares one direction.
    ///
    /// The property `renvor-sqlx`'s row-value seek depends on, asserted here at the source rather
    /// than only where it is consumed.
    #[test]
    fn a_resolved_ordering_never_mixes_directions() {
        for direction in [Direction::Ascending, Direction::Descending] {
            let order = allowlist()
                .order_by(&[("title".to_owned(), direction)])
                .expect("builds");
            let first = order.terms()[0].direction();
            assert!(
                order.terms().iter().all(|term| term.direction() == first),
                "a single-term sort resolved to a mixed ordering: {order:?}"
            );
        }
    }

    #[test]
    fn the_tiebreaker_is_not_duplicated_when_the_caller_sorted_by_it() {
        let order = allowlist()
            .order_by(&[("id".to_owned(), Direction::Descending)])
            .expect_err("id is not on the allowlist as a sortable field");
        // `id` is the tiebreaker but was never declared sortable, so it is refused — deny by
        // default applies to the tiebreaker column too.
        assert_eq!(order.kind(), DatabaseErrorKind::Unclassified);

        let with_id = allowlist().allow("id", "id");
        let order = with_id
            .order_by(&[("id".to_owned(), Direction::Descending)])
            .expect("builds");
        assert_eq!(order.terms().len(), 1, "the tiebreaker is already present");
        assert_eq!(order.terms()[0].keyword(), "DESC");
    }

    #[test]
    fn a_field_outside_the_allowlist_is_refused() {
        let error = allowlist()
            .order_by(&[("password_hash".to_owned(), Direction::Ascending)])
            .expect_err("refused");
        // The refusal carries no field text, because `DatabaseError` has no field text can inhabit.
        assert!(!error.description().contains("password_hash"));
    }

    #[test]
    fn an_ordering_without_a_tiebreaker_cannot_be_built() {
        let no_tiebreaker = SortAllowlist::new().allow("title", "title");
        assert!(no_tiebreaker.order_by(&[]).is_err());
    }

    #[test]
    fn the_sort_term_count_is_bounded_at_the_boundary_and_boundary_plus_one() {
        let mut list = SortAllowlist::new().tiebreaker("id");
        let mut terms = Vec::new();
        for index in 0..=MAX_SORT_TERMS {
            let field = format!("f{index}");
            list = list.allow(field.clone(), "col");
            terms.push((field, Direction::Ascending));
        }
        assert!(list.order_by(&terms[..MAX_SORT_TERMS]).is_ok());
        assert!(list.order_by(&terms[..=MAX_SORT_TERMS]).is_err());
    }

    #[test]
    fn rendering_uses_the_supplied_quoting_rule_and_nothing_else() {
        let order = allowlist()
            .order_by(&[("title".to_owned(), Direction::Ascending)])
            .expect("builds");
        let rendered = order.render(|column| format!("\"{column}\""));
        assert_eq!(rendered, "\"title\" ASC, \"id\" ASC");
    }

    #[test]
    fn a_keyset_round_trips_through_a_cursor() {
        let keyset = Keyset::new(0xDEAD_BEEF, vec![b"hello".to_vec(), b"42".to_vec()]);
        let encoded = keyset.to_cursor().encode();
        let decoded = Keyset::from_cursor(&encoded, 0xDEAD_BEEF).expect("decodes");
        assert_eq!(decoded, keyset);
    }

    #[test]
    fn a_cursor_from_a_different_query_is_refused() {
        let keyset = Keyset::new(1, vec![b"x".to_vec()]);
        let encoded = keyset.to_cursor().encode();
        assert_eq!(
            Keyset::from_cursor(&encoded, 2),
            Err(KeysetError::ForeignQuery)
        );
    }

    #[test]
    fn an_empty_keyset_round_trips() {
        let keyset = Keyset::new(7, Vec::new());
        let encoded = keyset.to_cursor().encode();
        assert_eq!(Keyset::from_cursor(&encoded, 7).expect("decodes"), keyset);
    }

    /// Decoding must not panic on any input, which is C-15's stated property for the cursor and
    /// therefore this layer's too.
    #[test]
    fn decoding_never_panics() {
        let inputs = [
            "",
            "!!!!",
            "AAAA",
            "A",
            &"A".repeat(2000),
            "AQ",
            &Cursor::new(vec![0_u8; 9]).encode(),
            &Cursor::new(vec![0_u8; 10]).encode(),
            &Cursor::new({
                let mut bytes = vec![0_u8; 8];
                bytes.extend_from_slice(&u16::MAX.to_be_bytes());
                bytes
            })
            .encode(),
        ];
        for input in inputs {
            let _ = Keyset::from_cursor(input, 0);
        }
    }

    #[test]
    fn a_truncated_payload_is_refused_rather_than_read_past_its_end() {
        let mut bytes = 0_u64.to_be_bytes().to_vec();
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&99_u16.to_be_bytes());
        bytes.extend_from_slice(b"short");
        let encoded = Cursor::new(bytes).encode();
        assert_eq!(
            Keyset::from_cursor(&encoded, 0),
            Err(KeysetError::Malformed)
        );
    }

    #[test]
    fn an_over_long_term_count_is_refused_before_allocating_for_it() {
        let mut bytes = 0_u64.to_be_bytes().to_vec();
        bytes.extend_from_slice(&u16::MAX.to_be_bytes());
        let encoded = Cursor::new(bytes).encode();
        assert_eq!(
            Keyset::from_cursor(&encoded, 0),
            Err(KeysetError::Malformed)
        );
    }

    #[test]
    fn a_page_carries_its_items_and_its_resume_position() {
        let page = Page::new(vec![1, 2, 3], Some(Cursor::new(vec![1])));
        assert_eq!(page.items(), &[1, 2, 3]);
        assert!(page.next().is_some());
        assert_eq!(Page::<i32>::new(Vec::new(), None).next(), None);
    }
}

use crate::DatabaseKind;

// ── Rendering an ordering into SQL ──────────────────────────────────────────────────────
//
// MOVED HERE FROM `renvor-sqlx` IN PHASE 007, and the move is the point.
//
// These three functions import no driver type — the original file's `grep -c sqlx` was zero. They
// turn a `OrderBy` and a `DatabaseKind`, both defined above, into SQL text. Living in the adapter
// meant the second adapter could not reach them, and Phase 007's FR-034 requires SeaORM to produce
// *"identical ordering and cursors to the SQLx row"*.
//
// A review found that requirement evidenced by an argument — "shared types; no adapter-specific
// behaviour exists" — which was false while `renvor-sqlx/src/page.rs` existed and its SeaORM
// counterpart did not. Copying it would have made the argument true and the guarantee weak: two
// renderers agree until one is edited. Moving it makes "identical" a property of the build.

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
/// [`crate::SortAllowlist`]. There is no caller text in the output, and the values the
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
mod rendering_tests {
    use super::*;
    use crate::SortAllowlist;

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
