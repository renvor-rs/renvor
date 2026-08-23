//! Bounded, allowlisted collection reads — the public contract, with no storage behind it.
//!
//! # This module executes no query
//!
//! FR-042 forbids it. What is defined here is the **shape** of a collection read: what a caller
//! may ask for, which asks are refused, and what a resumable position looks like. Phase 006
//! connects that shape to storage. Nothing below opens a connection, builds a statement, or
//! depends on a persistence crate.
//!
//! Fixing the shape now is the point: a persistence phase that invented its own pagination would
//! produce a second public contract, and the first one would already be published.
//!
//! # One principle, applied six times
//!
//! **Declared allowlist, explicit refusal, bounded count.** Filters, sorts, includes, and field
//! selections all follow it, and so do page size and duplicate keys. The alternative in every case
//! is a silent choice — a clamp, a last-one-wins, an ignored parameter — and constitution
//! principle IV prohibits silent fallbacks.
//!
//! # Why ordering must be total
//!
//! A sort on a non-unique column leaves ties, and two pages can order tied rows differently. A row
//! is then skipped or repeated, and nothing in the response says so. [`SortPlan`] therefore always
//! ends with a **tiebreaker**, which is what makes the ordering total. It is not optional, because
//! an optional correctness property is one that is absent whenever nobody thought about it.

use std::collections::{BTreeMap, BTreeSet};

use renvor_error::{Location, Pointer};

use crate::cursor::Cursor;
use crate::reason::Reason;
use crate::schema::Issue;

/// A comparison a filter may use.
///
/// A **closed** set. An open-ended operator vocabulary is how a filter parameter becomes a query
/// language, and a query language on a public endpoint is an unbounded amount of work a caller
/// gets to choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum FilterOperator {
    /// Equal to.
    Eq,
    /// Not equal to.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal to.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal to.
    Ge,
}

impl FilterOperator {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
        }
    }

    /// Parses a wire spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        [Self::Eq, Self::Ne, Self::Lt, Self::Le, Self::Gt, Self::Ge]
            .into_iter()
            .find(|operator| operator.as_str() == text)
    }

    /// Every operator.
    pub const ALL: [Self; 6] = [Self::Eq, Self::Ne, Self::Lt, Self::Le, Self::Gt, Self::Ge];
}

/// Which way a sort runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    /// Ascending.
    Ascending,
    /// Descending.
    Descending,
}

/// One sort term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortTerm {
    /// The allowlisted field.
    pub field: String,
    /// Which way it runs.
    pub direction: Direction,
}

/// A total ordering: the caller's terms, then a tiebreaker that cannot tie.
///
/// The tiebreaker is appended even when the caller supplied no terms, so **every** page of every
/// collection is ordered totally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortPlan {
    /// The caller's terms, in the order given.
    pub terms: Vec<SortTerm>,
    /// The field that breaks every remaining tie. Never absent.
    pub tiebreaker: String,
}

/// Page-size bounds.
///
/// A request outside them is **refused**, not clamped. Clamping hands a caller a different result
/// from the one they asked for and says nothing about having done so, which is the class of silent
/// substitution `contracts/http-runtime.md` already refuses for request limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBounds {
    /// Used when the caller asks for no particular size.
    pub default: u32,
    /// The smallest acceptable size.
    pub minimum: u32,
    /// The largest acceptable size.
    pub maximum: u32,
}

impl Default for PageBounds {
    /// A default that is small enough to be safe and large enough to be useful.
    fn default() -> Self {
        Self {
            default: 25,
            minimum: 1,
            maximum: 100,
        }
    }
}

/// What one operation permits a caller to ask for.
#[derive(Clone, Debug)]
pub struct CollectionContract {
    page: PageBounds,
    filterable: BTreeMap<String, BTreeSet<FilterOperator>>,
    sortable: BTreeSet<String>,
    includable: BTreeSet<String>,
    selectable: BTreeSet<String>,
    tiebreaker: String,
    max_filter_terms: usize,
    max_sort_terms: usize,
    max_includes: usize,
    max_selected_fields: usize,
}

impl CollectionContract {
    /// A contract whose ordering is broken by `tiebreaker`, permitting nothing else yet.
    ///
    /// Everything is **deny by default**: a field is filterable, sortable, includable, or
    /// selectable only once it is named. Constitution principle VI requires deny-by-default, and a
    /// contract that started permissive would be one an author had to remember to close.
    #[must_use]
    pub fn new(tiebreaker: impl Into<String>) -> Self {
        Self {
            page: PageBounds::default(),
            filterable: BTreeMap::new(),
            sortable: BTreeSet::new(),
            includable: BTreeSet::new(),
            selectable: BTreeSet::new(),
            tiebreaker: tiebreaker.into(),
            max_filter_terms: 16,
            max_sort_terms: 4,
            max_includes: 8,
            max_selected_fields: 64,
        }
    }

    /// Sets the page-size bounds.
    #[must_use]
    pub const fn with_page_bounds(mut self, page: PageBounds) -> Self {
        self.page = page;
        self
    }

    /// Permits `field` to be filtered with `operators`.
    #[must_use]
    pub fn filterable(
        mut self,
        field: impl Into<String>,
        operators: impl IntoIterator<Item = FilterOperator>,
    ) -> Self {
        self.filterable
            .insert(field.into(), operators.into_iter().collect());
        self
    }

    /// Permits `field` to be sorted on.
    #[must_use]
    pub fn sortable(mut self, field: impl Into<String>) -> Self {
        self.sortable.insert(field.into());
        self
    }

    /// Permits `relation` to be included.
    #[must_use]
    pub fn includable(mut self, relation: impl Into<String>) -> Self {
        self.includable.insert(relation.into());
        self
    }

    /// Permits `field` to be selected sparsely.
    #[must_use]
    pub fn selectable(mut self, field: impl Into<String>) -> Self {
        self.selectable.insert(field.into());
        self
    }

    /// The page-size bounds.
    #[must_use]
    pub const fn page_bounds(&self) -> PageBounds {
        self.page
    }

    /// The filterable fields and their permitted operators.
    #[must_use]
    pub const fn filterable_fields(&self) -> &BTreeMap<String, BTreeSet<FilterOperator>> {
        &self.filterable
    }

    /// The sortable fields.
    #[must_use]
    pub const fn sortable_fields(&self) -> &BTreeSet<String> {
        &self.sortable
    }

    /// Parses and validates a query, reporting **every** violation.
    ///
    /// `pairs` are the raw query-string pairs **in arrival order**, including repeats — duplicate
    /// detection is impossible once they have been collapsed into a map, which is why this takes a
    /// slice rather than a map.
    ///
    /// # Errors
    ///
    /// Every [`Issue`] found. An unknown key's **text is not echoed**: a key that failed the
    /// allowlist is caller-chosen, and the pointer names the parameter family instead.
    pub fn parse(&self, pairs: &[(String, String)]) -> Result<CollectionQuery, Vec<Issue>> {
        let mut issues = Vec::new();
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();

        // DUPLICATES FIRST, over the raw pairs. Not last-one-wins, not first-one-wins: a silent
        // winner depends on ordering nobody wrote down, which is the argument `RouteRegistry`
        // makes for duplicate routes.
        for (key, _) in pairs {
            *seen.entry(key.as_str()).or_insert(0) += 1;
        }
        for (key, count) in &seen {
            if *count > 1 {
                issues.push(issue(query_pointer(key, self), Reason::DuplicateKey));
            }
        }

        let lookup = |name: &str| -> Option<&str> {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };

        // ---- page size ----
        let page_size = match lookup("page_size") {
            None => self.page.default,
            Some(text) => match text.parse::<u32>() {
                Ok(size) if size >= self.page.minimum && size <= self.page.maximum => size,
                Ok(_) => {
                    issues.push(issue("page_size".to_owned(), Reason::OutOfRange));
                    self.page.default
                }
                Err(_) => {
                    issues.push(issue("page_size".to_owned(), Reason::TypeMismatch));
                    self.page.default
                }
            },
        };

        // ---- cursor ----
        let cursor = match lookup("cursor") {
            None => None,
            Some(text) => match Cursor::decode(text) {
                Ok(cursor) => Some(cursor),
                Err(error) => {
                    issues.push(issue("cursor".to_owned(), error.reason()));
                    None
                }
            },
        };

        // ---- sort ----
        let mut terms = Vec::new();
        if let Some(text) = lookup("sort") {
            let requested: Vec<&str> = text.split(',').filter(|s| !s.is_empty()).collect();
            if requested.len() > self.max_sort_terms {
                issues.push(issue("sort".to_owned(), Reason::TooManyTerms));
            }
            for raw in requested.into_iter().take(self.max_sort_terms) {
                let (field, direction) = raw
                    .strip_prefix('-')
                    .map_or((raw, Direction::Ascending), |stripped| {
                        (stripped, Direction::Descending)
                    });
                if self.sortable.contains(field) {
                    // SAFE to echo: the field matched the allowlist, so it is the contract's word.
                    terms.push(SortTerm {
                        field: field.to_owned(),
                        direction,
                    });
                } else {
                    issues.push(issue("sort".to_owned(), Reason::NotAllowlisted));
                }
            }
        }

        // ---- includes ----
        let includes = self.allowlisted_list(
            lookup("include"),
            &self.includable,
            self.max_includes,
            "include",
            &mut issues,
        );

        // ---- sparse fields ----
        let fields = self.allowlisted_list(
            lookup("fields"),
            &self.selectable,
            self.max_selected_fields,
            "fields",
            &mut issues,
        );

        // ---- filters ----
        let mut filters = Vec::new();
        let filter_pairs: Vec<&(String, String)> = pairs
            .iter()
            .filter(|(key, _)| key.starts_with("filter[") && key.ends_with(']'))
            .collect();
        if filter_pairs.len() > self.max_filter_terms {
            issues.push(issue("filter".to_owned(), Reason::TooManyTerms));
        }
        for (key, value) in filter_pairs.into_iter().take(self.max_filter_terms) {
            let inner = &key["filter[".len()..key.len() - 1];
            // `filter[field]` means `eq`; `filter[field][op]` names the operator.
            let (field, operator_text) = inner
                .split_once("][")
                .map_or((inner, "eq"), |(field, operator)| (field, operator));

            let Some(permitted) = self.filterable.get(field) else {
                issues.push(issue("filter".to_owned(), Reason::NotAllowlisted));
                continue;
            };
            let Some(operator) = FilterOperator::parse(operator_text) else {
                issues.push(issue("filter".to_owned(), Reason::OperatorNotAllowed));
                continue;
            };
            if !permitted.contains(&operator) {
                issues.push(issue("filter".to_owned(), Reason::OperatorNotAllowed));
                continue;
            }
            filters.push(FilterTerm {
                field: field.to_owned(),
                operator,
                value: value.clone(),
            });
        }

        if issues.is_empty() {
            Ok(CollectionQuery {
                page_size,
                cursor,
                sort: SortPlan {
                    terms,
                    tiebreaker: self.tiebreaker.clone(),
                },
                filters,
                includes,
                fields,
            })
        } else {
            // Deterministic for the same input: sorted by pointer, then by reason.
            issues.sort_by(|left, right| {
                left.pointer
                    .as_str()
                    .cmp(right.pointer.as_str())
                    .then_with(|| left.reason.as_str().cmp(right.reason.as_str()))
            });
            issues.dedup();
            Err(issues)
        }
    }

    fn allowlisted_list(
        &self,
        raw: Option<&str>,
        allowlist: &BTreeSet<String>,
        bound: usize,
        parameter: &'static str,
        issues: &mut Vec<Issue>,
    ) -> Vec<String> {
        let Some(text) = raw else {
            return Vec::new();
        };
        let requested: Vec<&str> = text.split(',').filter(|s| !s.is_empty()).collect();
        if requested.len() > bound {
            issues.push(issue(parameter.to_owned(), Reason::TooManyTerms));
        }
        let mut accepted = Vec::new();
        for name in requested.into_iter().take(bound) {
            if allowlist.contains(name) {
                accepted.push(name.to_owned());
            } else {
                issues.push(issue(parameter.to_owned(), Reason::NotAllowlisted));
            }
        }
        accepted
    }
}

/// Which query-string family a key belongs to, for pointing at a duplicate without echoing it.
fn query_pointer(key: &str, _contract: &CollectionContract) -> String {
    if key.starts_with("filter[") {
        "filter".to_owned()
    } else if matches!(key, "page_size" | "cursor" | "sort" | "include" | "fields") {
        // A RESERVED name, so it is this contract's word rather than the caller's.
        key.to_owned()
    } else {
        // An unrecognised key. Not echoed.
        "query".to_owned()
    }
}

fn issue(pointer: String, reason: Reason) -> Issue {
    Issue {
        location: Location::Query,
        pointer: Pointer::new(pointer)
            .unwrap_or_else(|_| Pointer::new("query").expect("a constant pointer")),
        reason,
    }
}

/// One accepted filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterTerm {
    /// The allowlisted field.
    pub field: String,
    /// The permitted operator.
    pub operator: FilterOperator,
    /// The caller's value, **uninterpreted**.
    ///
    /// Phase 006 binds this as a parameter. It is never concatenated into a statement — constitution
    /// principle VI requires parameter binding, and a value that reached a statement by string
    /// concatenation would be an injection whichever phase performed it.
    pub value: String,
}

/// A validated collection read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionQuery {
    /// The page size, within bounds.
    pub page_size: u32,
    /// Where to resume, if the caller supplied a cursor.
    pub cursor: Option<Cursor>,
    /// The total ordering.
    pub sort: SortPlan,
    /// The accepted filters.
    pub filters: Vec<FilterTerm>,
    /// The accepted includes.
    pub includes: Vec<String>,
    /// The accepted sparse-field selections.
    pub fields: Vec<String>,
}
