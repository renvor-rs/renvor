//! Authentication, sessions, tokens, and authorization policies for the Renvor framework.
//!
//! # This crate names no transport
//!
//! A policy decision is a property of an **application operation**, not of the protocol that
//! reached it. FR-057 requires the check to live inside the operation, and FR-061 requires that a
//! transport adapter cannot bypass it. Both are structural properties of where this code lives:
//! there is no router here, no status code, and no database driver.
//!
//! # Three guarantees that hold by construction
//!
//! `renvor-error` established the pattern this crate follows — redaction is a property of the
//! types, not a rule reviewers enforce:
//!
//! - [`Opaque`] has no `Serialize`, no `Deref`, no `PartialEq`, and its `Debug` and `Display` both
//!   render a placeholder. Printing one is not a mistake that review has to catch, because there is
//!   no code path that prints it.
//! - [`Subject`] is a two-variant enum, not an `Option<UserId>`. Reaching a user identity means
//!   handling the anonymous case; there is no `unwrap` shortcut because there is no `Option`.
//! - [`Clock`] is a port. Expiry is evaluated against an injected instant, so a test moves time
//!   instead of waiting for it, and production reads the real clock through the same trait.

pub mod clock;
pub mod cookie;
pub mod error;
pub mod mail;
pub mod opaque;
pub mod password;
pub mod repository;
pub mod service;
pub mod session;
pub mod subject;

pub use clock::{Clock, FixedClock, SystemClock};
pub use cookie::{CookiePolicy, CookieRejection, SameSiteChoice, SetCookie};
pub use error::AuthError;
pub use mail::{MailError, MailKind, MailPort, OutgoingMail, RecordingMailSink};
pub use opaque::{Opaque, OpaqueKind, SecretDigest};
pub use password::{PasswordHash, PasswordPolicy, PasswordService};
pub use repository::{
    CredentialRecord, CredentialRepository, Registration, SingleUseTokenRepository, UserRecord,
    UserRepository,
};
pub use service::{Authenticated, AuthenticationService, ServiceError};
pub use session::{
    Established, LogoutOutcome, SessionOutcome, SessionPolicy, SessionRecord, SessionRejection,
    SessionRepository, SessionService,
};
pub use subject::{AuthenticatedSubject, Subject, UserId};
