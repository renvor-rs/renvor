//! Fixtures and factories (Phase 011, FR-049): deterministic test data, driver-free.
//!
//! # Determinism is the point
//!
//! A factory draws every value from a [`Sequence`] the test owns. Two runs with the same seed
//! produce the same emails, names, and passwords, so a failure reproduces; two factories sharing
//! one sequence never collide, so a test that registers "the next user" twice registers two users.
//! The sequence can be seeded from a literal, or from an injected
//! [`EntropySource`](renvor_core::observe::entropy::EntropySource), which is the
//! same source the application under test draws from.
//!
//! Nothing here names a driver, a transport, or a table: a draft is plain data the caller sends
//! wherever the test sends it — over a socket, through a dispatched request, or into a
//! repository.

use renvor_core::observe::entropy::EntropySource;

/// A deterministic counter every factory draws from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    seed: u64,
    next: u64,
}

impl Sequence {
    /// Starts at `seed`; the first value drawn is `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { seed, next: seed }
    }

    /// Seeds from eight bytes of `entropy` — the injected source, so a test that fixes it fixes
    /// every draft too.
    ///
    /// # Errors
    ///
    /// The source's own failure, unchanged.
    pub fn from_entropy(
        entropy: &dyn EntropySource,
    ) -> Result<Self, renvor_core::observe::entropy::EntropyUnavailable> {
        let mut bytes = [0_u8; 8];
        entropy.fill(&mut bytes)?;
        Ok(Self::new(u64::from_le_bytes(bytes)))
    }

    /// The seed this sequence started from, for a failure message.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// The next value; wraps rather than panics, since a test never draws that many.
    pub fn draw(&mut self) -> u64 {
        let value = self.next;
        self.next = self.next.wrapping_add(1);
        value
    }
}

/// Builds one `T` per draw from a [`Sequence`].
pub trait Factory<T> {
    /// Builds the next value.
    fn build(&self, sequence: &mut Sequence) -> T;
}

/// A registration: an address and a password that passes the default policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDraft {
    /// `user-<n>@<domain>`.
    pub email: String,
    /// Long enough for the default password policy, distinct per user.
    pub password: String,
}

/// Users under one domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFactory {
    domain: String,
}

impl UserFactory {
    /// Users under `domain`, such as `example.test`.
    #[must_use]
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
        }
    }
}

impl Factory<UserDraft> for UserFactory {
    fn build(&self, sequence: &mut Sequence) -> UserDraft {
        let n = sequence.draw();
        UserDraft {
            email: format!("user-{n:x}@{}", self.domain),
            password: format!("correct-horse-battery-staple-{n:x}"),
        }
    }
}

/// An item of the example domain: a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDraft {
    /// `item-<n>`, within the column's bound.
    pub name: String,
}

/// Items with a fixed prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFactory {
    prefix: String,
}

impl ItemFactory {
    /// Items named `<prefix>-<n>`.
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl Default for ItemFactory {
    fn default() -> Self {
        Self::new("item")
    }
}

impl Factory<ItemDraft> for ItemFactory {
    fn build(&self, sequence: &mut Sequence) -> ItemDraft {
        ItemDraft {
            name: format!("{}-{:x}", self.prefix, sequence.draw()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renvor_core::observe::entropy::FixedEntropy;

    #[test]
    fn the_same_seed_builds_the_same_drafts_and_one_sequence_never_repeats() {
        let users = UserFactory::new("example.test");
        let mut first = Sequence::new(0x9f3a);
        let mut second = Sequence::new(0x9f3a);
        let a = users.build(&mut first);
        let b = users.build(&mut second);
        assert_eq!(a, b, "determinism");
        assert_eq!(a.email, "user-9f3a@example.test");
        let c = users.build(&mut first);
        assert_ne!(a.email, c.email, "one sequence, two users");
        assert_ne!(a.password, c.password);
        assert!(a.password.len() >= 20, "long enough for the default policy");
        let items = ItemFactory::default();
        assert_eq!(items.build(&mut first).name, "item-9f3c");
        assert_eq!(first.seed(), 0x9f3a);
    }

    #[test]
    fn a_sequence_seeded_from_fixed_entropy_is_fixed() {
        let entropy = FixedEntropy::new([7; 32]);
        let one = Sequence::from_entropy(&entropy).expect("entropy");
        let two = Sequence::from_entropy(&entropy).expect("entropy");
        assert_eq!(one, two);
        assert_eq!(one.seed(), u64::from_le_bytes([7; 8]));
    }
}
