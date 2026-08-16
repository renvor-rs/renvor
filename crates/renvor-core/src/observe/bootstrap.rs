//! Installing a tracing subscriber — **the author's decision, never the kernel's**.
//!
//! # What this module deliberately does not do
//!
//! It does not install anything. `ApplicationBuilder::build` installs nothing, `boot` installs
//! nothing, and no constructor in this crate touches the process-global subscriber.
//!
//! Contract C-O7 and research D4 both say why: the global subscriber is a **process-wide, once-per-
//! process** decision. A library that takes it takes it away from every consumer that depends on
//! that library — including consumers who never asked for tracing, and consumers who already
//! installed their own. Worse, it takes it *silently*, at whatever moment the library happens to
//! initialise, which is not a moment the application author chose.
//!
//! # Why a helper exists anyway
//!
//! Refusing to help is its own failure mode: an author who has to remember the exact incantation
//! writes it slightly differently in every binary. [`try_init_global`] does the one thing that is
//! genuinely awkward to get right — reporting the second call **as an error** rather than
//! panicking or silently doing nothing.
//!
//! Both alternatives are worse in the same way:
//!
//! | Behaviour on a second call | Why it is wrong |
//! |---|---|
//! | panic | a diagnostic decision takes down a running process |
//! | silently succeed | the caller believes their subscriber is installed; it is not |
//! | silently replace | the caller who installed first loses their records without being told |
//!
//! FR-029 permits **0** of the three. So this returns [`AlreadyInstalled`], and the caller decides.

use core::fmt;

use tracing::Subscriber;

/// A global subscriber was already installed, so this call installed nothing.
///
/// Deliberately its own type rather than a `bool` or a `KernelError`: it is neither a kernel
/// defect nor an author-facing configuration problem, and forcing it into the taxonomy would make
/// it look like one of those.
#[derive(Debug, PartialEq, Eq)]
pub struct AlreadyInstalled;

impl fmt::Display for AlreadyInstalled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "a global tracing subscriber is already installed; this call installed nothing and \
             replaced nothing",
        )
    }
}

impl std::error::Error for AlreadyInstalled {}

/// Installs `subscriber` as the process-global default, if nothing else has claimed it.
///
/// # This is process-wide and happens once
///
/// The global subscriber can be set **once per process**. Calling this from a library — including
/// from a Renvor provider — takes the decision away from the application author. It belongs in
/// `main`, and nowhere else.
///
/// # Errors
///
/// Returns [`AlreadyInstalled`] if a subscriber is already installed. Nothing is replaced and
/// nothing is dropped; the existing subscriber keeps receiving records.
pub fn try_init_global<S>(subscriber: S) -> Result<(), AlreadyInstalled>
where
    S: Subscriber + Send + Sync + 'static,
{
    tracing::subscriber::set_global_default(subscriber).map_err(|_| AlreadyInstalled)
}

#[cfg(test)]
mod tests {
    use super::{AlreadyInstalled, try_init_global};

    #[test]
    fn the_error_says_plainly_that_nothing_was_replaced() {
        // The message is the whole product of this type: a caller who reads "already installed"
        // and assumes their subscriber won is exactly who this is for.
        let rendered = AlreadyInstalled.to_string();
        assert!(rendered.contains("installed nothing"), "{rendered}");
        assert!(rendered.contains("replaced nothing"), "{rendered}");
    }

    #[test]
    fn a_second_install_is_an_error_rather_than_a_panic_or_a_silent_success() {
        // FR-029: 0 panics, 0 silent successes, 0 silent replacements. Run in one test because the
        // global subscriber is process-wide — two tests would race for the single slot and the
        // loser would report a failure that is really a scheduling accident.
        let first = try_init_global(tracing::subscriber::NoSubscriber::default());
        assert!(first.is_ok(), "the first install claims the slot");

        let second = try_init_global(tracing::subscriber::NoSubscriber::default());
        assert_eq!(
            second,
            Err(AlreadyInstalled),
            "the second must report, not panic and not pretend"
        );
    }
}
