//! OpenAPI 3.2.0 description generation and API compatibility checking for the Renvor framework.
//!
//! # Why this crate serialises the document itself
//!
//! Constitution principle V is binding and specific:
//!
//! > *"REST MUST use the current approved OpenAPI standard … The initial target is OpenAPI 3.2.0,
//! > and emitted documents MUST NOT claim a version that selected tooling does not correctly
//! > implement."*
//!
//! OAS 3.2.0 was released on 2025-09-19. On 2026-08-23 the Rust ecosystem was re-evaluated against
//! primary sources and **every** maintained generator emits an earlier version — `utoipa` 5.5.0
//! emits `"3.1.0"`, measured by compiling a `#[derive(OpenApi)]` type and printing the field
//! rather than by reading its documentation. Relabelling that output as 3.2 is precisely what
//! principle V forbids.
//!
//! The responsibility taken on here is **bounded**, and the bound is the point:
//!
//! | | |
//! |---|---|
//! | **It does** | emit the document envelope and its operations, deterministically |
//! | **It does not** | implement JSON Schema — `schemars` does |
//! | | parse documents written by anyone else |
//! | | resolve remote references |
//! | | judge validity — the **official** schema does that, checked by an independent validator |
//!
//! See ADR-0013 for the evaluated packages, their concrete shortcomings, the ownership cost, and
//! the deletion trigger.
//!
//! # The 3.2 gate is fail-closed, and it does not trust this crate
//!
//! `tests/openapi_3_2_gate.rs` runs five proofs against the **vendored official schemas**:
//!
//! 1. the document declares `3.2.0`;
//! 2. it validates against the official OpenAPI **3.2** schema;
//! 3. it is rejected by the official **3.1** schema **with the version constraint neutralised** —
//!    so the rejection is structural, not a version-string mismatch, which is what proves the
//!    output is not relabelled 3.1;
//! 4. a relabelled 3.1 document **passes** that same neutralised check — the control proving the
//!    discriminator discriminates;
//! 5. malformed documents are rejected — the controls proving the validator is not vacuous.
//!
//! # This crate carries no runtime server state
//!
//! Generating a description binds no socket, opens no connection, runs no migration, and boots no
//! provider. It depends on no server, router, or middleware crate.
//!
//! # Stability
//!
//! This surface is **explicitly unstable**. See
//! [`contracts/api-stability.md`](https://github.com/renvor-rs/renvor/blob/main/contracts/api-stability.md).

pub mod compat;
pub mod document;

pub use compat::{Change, ChangeClass, Severity, breaking, compare, is_breaking};
pub use document::{
    Components, Document, DocumentError, Info, JSON_MEDIA_TYPE, MediaType, OPENAPI_DIALECT,
    OPENAPI_VERSION, Operation, PROBLEM_COMPONENT, Parameter, ParameterLocation, PathItem,
    RequestBody, Response, Server, Tag, problem_schema,
};

use renvor_validation::Declaration;

/// Validates every declared example against its own schema.
///
/// # Why this is an error rather than a warning
///
/// An example that contradicts its schema is worse than no example, because consumers copy
/// examples. A warning is a thing a build ignores.
///
/// The validator is Renvor's own [`Declaration`] — the **same** interpreter that enforces the
/// schema at runtime. Using a second validator here would allow two opinions about one question,
/// and the whole shape of this phase is refusing that.
///
/// # Errors
///
/// [`DocumentError::ExampleInvalid`] naming where the offending example was declared. The example
/// itself is **not** included in the message: an example is author data, and this error can reach
/// a log.
pub fn validate_examples(document: &Document) -> Result<(), DocumentError> {
    for (path, item) in &document.paths {
        for (method, operation) in item.operations() {
            if let Some(body) = &operation.request_body {
                for (media_type, content) in &body.content {
                    check_example(
                        content,
                        &format!("{method} {path} requestBody {media_type}"),
                    )?;
                }
            }
            for (status, response) in &operation.responses {
                for (media_type, content) in &response.content {
                    check_example(
                        content,
                        &format!("{method} {path} response {status} {media_type}"),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn check_example(content: &MediaType, at: &str) -> Result<(), DocumentError> {
    let Some(example) = &content.example else {
        return Ok(());
    };
    // A schema that is not a declaration cannot be checked, and silently skipping it would make
    // this function's guarantee conditional on something the caller cannot see.
    let declaration = Declaration::new(content.schema.clone())
        .map_err(|_| DocumentError::ExampleInvalid { at: at.to_owned() })?;

    if declaration.is_valid(example) {
        Ok(())
    } else {
        Err(DocumentError::ExampleInvalid { at: at.to_owned() })
    }
}
