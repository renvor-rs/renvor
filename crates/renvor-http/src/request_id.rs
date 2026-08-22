//! Request identifiers that are **always generated**.
//!
//! # Why Renvor does not use the middleware collection's version
//!
//! ADR-0012, Finding 1. `tower_http::request_id::SetRequestId` adopts an inbound header as the
//! request identity and generates one **only when the header is absent**, and the crate exposes no
//! overwrite option:
//!
//! ```text
//! if let Some(request_id) = req.headers().get(&self.header_name) {
//!     ...insert(RequestId::new(request_id.clone()));      // the CALLER's value becomes identity
//! } else if let Some(request_id) = self.make_request_id.make_request_id(&req) {
//! ```
//!
//! A caller therefore chooses the value under which its own request is logged, correlated, and
//! audited — including one that collides with another tenant's, which turns a log search into a
//! false lead at exactly the wrong moment.
//!
//! # What this module does instead
//!
//! Generates on **every** request, from the entropy source and nothing else. An inbound value is
//! never adopted, never echoed, and never stored anywhere it could be mistaken for the generated
//! one — [`RequestId`] has no constructor that accepts a header, so that is a property of the type.

use renvor_core::{EntropySource, observe::entropy::EntropyUnavailable};

use crate::context::RequestId;

/// The header the generated identifier is published under.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Generates an identifier from `source` and **nothing else**.
///
/// The single generation site, so reviewing opacity means reviewing one function — the same
/// discipline `RunIdentifier::generate` follows.
///
/// # Errors
///
/// Returns [`EntropyUnavailable`] if the source cannot supply bytes. This **propagates** rather
/// than falling back to a weaker source: an identifier from a predictable source is worse than no
/// identifier, because it looks exactly like a good one.
pub fn generate(source: &dyn EntropySource) -> Result<RequestId, EntropyUnavailable> {
    let mut bytes = [0_u8; 8];
    source.fill(&mut bytes)?;
    Ok(RequestId::from_entropy(bytes))
}

#[cfg(test)]
mod tests {
    use super::{REQUEST_ID_HEADER, generate};
    use renvor_core::OsEntropy;

    #[test]
    fn generation_does_not_consult_any_inbound_value() {
        // The signature is the assertion: `generate` takes an entropy source and nothing else.
        // There is no parameter through which a header could reach it, so "the inbound value is
        // ignored" is not a behaviour that could regress — there is nowhere to pass it.
        let first = generate(&OsEntropy).expect("entropy is available");
        let second = generate(&OsEntropy).expect("entropy is available");
        assert_ne!(
            first.encode(),
            second.encode(),
            "two generated identifiers collided, which means the source is not varying"
        );
    }

    #[test]
    fn the_encoded_form_is_sixteen_lowercase_hex_characters() {
        let id = generate(&OsEntropy).expect("entropy is available");
        let encoded = id.encode();
        assert_eq!(encoded.len(), 16);
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "not lowercase hex: {encoded}"
        );
    }

    #[test]
    fn the_published_header_name_is_lowercase() {
        // Header names are case-insensitive on the wire, but a lowercase constant means every
        // comparison in this crate is against one spelling.
        assert_eq!(REQUEST_ID_HEADER, "x-request-id");
        assert_eq!(REQUEST_ID_HEADER, REQUEST_ID_HEADER.to_ascii_lowercase());
    }
}
