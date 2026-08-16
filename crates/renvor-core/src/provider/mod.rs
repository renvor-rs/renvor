//! Providers: registration, dependency declaration, and single-pass resolution.
//!
//! A provider is a unit of application capability that may depend on other capabilities. The
//! kernel's job here is narrow and entirely deterministic: given a set of registered providers and
//! their declared dependencies, decide an initialisation order, or refuse with a diagnostic that
//! names every provider involved in the refusal.
//!
//! The resolution machinery lives in [`graph`]. It is separated from registration because the two
//! answer different questions — registration decides what is *declared*, resolution decides what
//! that declaration *means* — and because the resolver carries a counted work budget that must be
//! observable on its own.

pub mod graph;
