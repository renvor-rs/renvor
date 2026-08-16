//! Typed application state, keyed by type and carrying each entry's type name.
//!
//! # This is custom infrastructure, and it is recorded as such
//!
//! ADR-0007 Primitive 1. `anymap3`, `http::Extensions`, and `state` were evaluated and rejected;
//! the decisive reason is metadata colocation, not licence or maintenance. See that record for the
//! full justification, the exit trigger, and the replacement strategy.
//!
//! **The short version**: an any-map is keyed by type and returns the value. It has nowhere to
//! store the type *name*, which FR-011 needs to report a duplicate registration and FR-037(b)
//! permits the kernel to emit. Adopting one therefore means maintaining a **second parallel map**
//! of names in lockstep with the first — two structures that can disagree — wrapping something
//! that is otherwise a `std` `HashMap`. The custom version is smaller than the adoption.
//!
//! # Opaque state is treated as sensitive
//!
//! FR-037(b) classifies author-registered state as one of exactly two sensitive classes, and is
//! explicit that the kernel **must not assume opaque state is safe to print merely because it was
//! not marked secret** — an author may register a credential without marking anything.
//!
//! So the private `StateEntry` stores the value as `Box<dyn Any + Send + Sync>`, which has **no
//! `Debug` bound at all**. The kernel therefore *cannot* print a stored value: it is not a rule
//! that could be broken by a future `{:?}`, it is an absent trait. The hand-written [`fmt::Debug`]
//! impl on [`TypedStateMap`] emits type names only, and a test asserts a registered credential's
//! contents appear in **0** of the map's output forms.
//!
//! # Errors, never panics
//!
//! FR-010 requires a missing type to be an error rather than a runtime panic, and SC-004 asserts
//! **0** panics in ordinary use. Both [`TypedStateMap::insert`] and [`TypedStateMap::get`] return
//! `Result`; neither unwraps, indexes, or asserts.

use core::any::{Any, TypeId, type_name};
use core::fmt;
use std::collections::HashMap;

use crate::error::KernelError;

/// One registered value together with the name of its type.
///
/// The name is stored **beside** the value rather than derived on demand, because `TypeId` cannot
/// be turned back into a name — recovering it later is impossible, so it is captured at insert
/// time when the concrete type is still known.
struct StateEntry {
    /// Deliberately `dyn Any` with **no `Debug` bound**. See the module documentation: this is
    /// what makes "the kernel cannot print your state" a property of the type rather than a
    /// convention.
    value: Box<dyn Any + Send + Sync>,
    type_name: &'static str,
}

/// Typed application state, retrievable by type.
///
/// Registration is by concrete type: one value per type, and a second registration of the same
/// type is an error naming it (FR-011).
#[derive(Default)]
pub struct TypedStateMap {
    entries: HashMap<TypeId, StateEntry>,
}

impl TypedStateMap {
    /// Creates an empty state map.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Registers `value` under its own type.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::StateDuplicate`] naming the type if a value of this type is already
    /// registered (FR-011). The existing value is left untouched — a duplicate registration is a
    /// programming error, and silently replacing would make the winner depend on registration
    /// order that nobody wrote down.
    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) -> Result<(), KernelError> {
        let key = TypeId::of::<T>();
        let name = type_name::<T>();

        if self.entries.contains_key(&key) {
            return Err(KernelError::StateDuplicate { type_name: name });
        }

        self.entries.insert(
            key,
            StateEntry {
                value: Box::new(value),
                type_name: name,
            },
        );
        Ok(())
    }

    /// Retrieves the registered value of type `T`.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::StateMissing`] naming the type if nothing of this type was
    /// registered (FR-010). Never panics — SC-004 asserts 0 panics in ordinary use.
    pub fn get<T: Any + Send + Sync>(&self) -> Result<&T, KernelError> {
        let name = type_name::<T>();
        self.entries
            .get(&TypeId::of::<T>())
            .and_then(|entry| entry.value.downcast_ref::<T>())
            .ok_or(KernelError::StateMissing { type_name: name })
    }

    /// Whether a value of type `T` is registered.
    #[must_use]
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.entries.contains_key(&TypeId::of::<T>())
    }

    /// How many values are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The type names of every registered value, in unspecified order.
    ///
    /// Names only — never values. This is the entire diagnostic surface FR-037(b) permits.
    pub fn type_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.values().map(|entry| entry.type_name)
    }
}

/// Emits **type names only**, never values.
///
/// Written by hand rather than derived. A derived impl would require `StateEntry: Debug`, which
/// would require `Box<dyn Any + Send + Sync>: Debug`, which does not hold — so the derive would
/// not compile. That is the intended outcome: the safe impl is the only one available.
impl fmt::Debug for TypedStateMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names: Vec<&str> = self.type_names().collect();
        names.sort_unstable();
        f.debug_struct("TypedStateMap")
            .field("registered", &names)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::TypedStateMap;
    use crate::error::ErrorCategory;

    #[derive(Debug, PartialEq)]
    struct Db(u16);

    #[derive(Debug, PartialEq)]
    struct Cache(&'static str);

    /// A credential-bearing value registered **without** being marked secret — the exact case
    /// FR-037(b) says the kernel must not assume is safe to print.
    struct ApiCredential {
        #[allow(dead_code)]
        token: String,
    }

    #[test]
    fn a_registered_value_is_retrievable_by_type() {
        let mut state = TypedStateMap::new();
        state.insert(Db(5432)).expect("first registration succeeds");
        state
            .insert(Cache("redis"))
            .expect("a different type is fine");

        assert_eq!(state.get::<Db>().unwrap(), &Db(5432));
        assert_eq!(state.get::<Cache>().unwrap(), &Cache("redis"));
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn a_duplicate_registration_errors_and_names_the_type() {
        // FR-011.
        let mut state = TypedStateMap::new();
        state.insert(Db(1)).unwrap();
        let error = state
            .insert(Db(2))
            .expect_err("a second value of the same type must be rejected");

        assert_eq!(error.category(), ErrorCategory::StateDuplicate);
        assert!(error.to_string().contains("Db"), "{error}");

        // The original survives. Silently replacing would make the winner depend on registration
        // order nobody wrote down.
        assert_eq!(state.get::<Db>().unwrap(), &Db(1));
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn retrieving_an_unregistered_type_errors_rather_than_panicking() {
        // FR-010 and SC-004: an error, never a runtime panic.
        let state = TypedStateMap::new();
        let error = state
            .get::<Db>()
            .expect_err("an unregistered type must not be retrievable");

        assert_eq!(error.category(), ErrorCategory::StateMissing);
        assert!(error.to_string().contains("Db"), "{error}");
    }

    #[test]
    fn distinct_types_do_not_collide_even_with_identical_shapes() {
        // Both are newtypes over the same primitive. Keying by TypeId rather than by layout is
        // what keeps them apart; a value-shaped key would alias them.
        #[derive(Debug, PartialEq)]
        struct Port(u16);
        #[derive(Debug, PartialEq)]
        struct Timeout(u16);

        let mut state = TypedStateMap::new();
        state.insert(Port(8080)).unwrap();
        state.insert(Timeout(8080)).unwrap();

        assert_eq!(state.get::<Port>().unwrap(), &Port(8080));
        assert_eq!(state.get::<Timeout>().unwrap(), &Timeout(8080));
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn state_contents_appear_in_zero_output_forms() {
        // SC-016 / FR-037(b). The registered value carries a credential and was NOT marked
        // secret, because the requirement is that the kernel must not assume unmarked state is
        // safe to print.
        let mut state = TypedStateMap::new();
        state
            .insert(ApiCredential {
                token: "hunter2-do-not-print".to_owned(),
            })
            .unwrap();

        let debug_output = format!("{state:?}");
        assert!(
            !debug_output.contains("hunter2-do-not-print"),
            "the credential reached Debug output: {debug_output}"
        );

        // The error paths are the other output forms that could leak.
        let duplicate = state
            .insert(ApiCredential {
                token: "hunter2-do-not-print".to_owned(),
            })
            .unwrap_err();
        assert!(!duplicate.to_string().contains("hunter2-do-not-print"));

        let missing = state.get::<Db>().unwrap_err();
        assert!(!missing.to_string().contains("hunter2-do-not-print"));

        // POSITIVE CONTROL: the search string is findable when it IS present, so the three
        // assertions above are discriminating rather than vacuous.
        assert!(format!("{:?}", "hunter2-do-not-print").contains("hunter2-do-not-print"));
    }

    #[test]
    fn debug_output_carries_type_names_and_only_type_names() {
        // FR-037(b) permits the type name. This asserts the permitted thing is actually present —
        // a Debug impl that emitted nothing would pass the leak test above for the wrong reason.
        let mut state = TypedStateMap::new();
        state.insert(Db(5432)).unwrap();
        state.insert(Cache("redis")).unwrap();

        let output = format!("{state:?}");
        assert!(output.contains("Db"), "{output}");
        assert!(output.contains("Cache"), "{output}");
        assert!(
            !output.contains("5432") && !output.contains("redis"),
            "values leaked into Debug: {output}"
        );
    }

    #[test]
    fn no_production_path_in_this_module_can_panic() {
        // SC-004 requires **0** panics in ordinary use. The tests above prove the two error paths
        // return errors; this proves there is no *third* path that ends the process instead —
        // including one added later by an edit that never runs in any existing test.
        //
        // The scan reads this module's own source and stops at the test module, because tests are
        // allowed to unwrap: a test that cannot fail loudly is not a test.
        let source = include_str!("mod.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields at least one part");

        for construct in [
            ".unwrap()",
            ".expect(",
            "panic!",
            "unreachable!",
            "todo!",
            "unimplemented!",
        ] {
            assert!(
                !production.contains(construct),
                "`{construct}` reached the production half of the typed state map, \
                 which SC-004 requires to have 0 panicking paths"
            );
        }

        // POSITIVE CONTROL: the same search finds these constructs in the test half, so its
        // silence above means "absent", not "the search is broken".
        let tests = &source[production.len()..];
        assert!(tests.contains(".unwrap()"), "the control text moved");
        assert!(tests.contains(".expect("), "the control text moved");
    }

    #[test]
    fn contains_and_len_track_registration() {
        let mut state = TypedStateMap::new();
        assert!(state.is_empty());
        assert!(!state.contains::<Db>());

        state.insert(Db(1)).unwrap();
        assert!(state.contains::<Db>());
        assert!(
            !state.contains::<Cache>(),
            "and discriminates between types"
        );
        assert_eq!(state.len(), 1);
        assert!(!state.is_empty());
    }
}
