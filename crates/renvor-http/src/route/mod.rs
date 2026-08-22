//! Routes, groups, and the transport-neutral handler seam.
//!
//! # Where HTTP stops
//!
//! A handler receives a [`Request`] and returns a [`Response`]. Both are **Renvor types**. Neither
//! names `axum`, `tower`, or `http`, and neither can — they are declared here, in a module that
//! imports none of them.
//!
//! That is the whole boundary, and it is enforced by absence rather than by review: a signature
//! cannot leak a type the module never imports. `tests/boundary.rs` asserts this by scanning the
//! source, with a positive control proving the scan finds a transport type when one is present.
//!
//! # Why the handler is a trait object rather than a generic
//!
//! Routes are collected into one registry at runtime, so their handlers must share a type. A
//! generic handler would make [`RouteRegistry`] generic over every route it holds, which is not a
//! type that can exist. The cost is one allocation and one virtual call per request, which is
//! nothing beside the syscalls either side of it.

pub mod build;
pub mod inspect;
pub mod registry;

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::context::RequestContext;

pub use registry::{RouteError, RouteRegistry};

/// The request methods Renvor routes on.
///
/// A closed enum rather than a string. A string method would let `"get"` and `"GET"` register as
/// two routes, and the duplicate detection this crate promises would then be false for the case it
/// most needs to catch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `PATCH`
    Patch,
    /// `DELETE`
    Delete,
    /// `HEAD`
    Head,
    /// `OPTIONS`
    Options,
}

impl Method {
    /// The canonical uppercase spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }

    /// Every method, in the order used for a deterministic `Allow` header.
    #[must_use]
    pub const fn all() -> [Self; 7] {
        [
            Self::Get,
            Self::Post,
            Self::Put,
            Self::Patch,
            Self::Delete,
            Self::Head,
            Self::Options,
        ]
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a handler receives.
///
/// Deliberately small. Everything a handler needs about *who is asking* lives in
/// [`RequestContext`], which is the value the security layers have already resolved — a handler
/// never re-derives client identity from a header, because it never sees the headers that would
/// let it.
pub struct Request {
    context: RequestContext,
    body: Vec<u8>,
    query: String,
    path_params: BTreeMap<String, String>,
}

impl Request {
    /// Builds a request. Used by the router adapter and by tests.
    #[must_use]
    pub fn new(
        context: RequestContext,
        body: Vec<u8>,
        query: String,
        path_params: BTreeMap<String, String>,
    ) -> Self {
        Self {
            context,
            body,
            query,
            path_params,
        }
    }

    /// The resolved request context: run id, request id, client identity, host, cancellation.
    #[must_use]
    pub const fn context(&self) -> &RequestContext {
        &self.context
    }

    /// The request body bytes, already bounded by the configured body limit.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The raw query string, without the leading `?`.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// A path parameter captured by the route pattern.
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }
}

/// A `Debug` that cannot print a body.
///
/// A request body is author data of unknown sensitivity — the same reasoning `TypedStateMap`
/// applies to registered state. Printing its length is a fact; printing its content is a
/// disclosure nobody reviewed.
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("context", &self.context)
            .field("body_len", &self.body.len())
            .field("path_params", &self.path_params.len())
            .finish_non_exhaustive()
    }
}

/// What a handler returns.
#[derive(Clone, PartialEq, Eq)]
pub struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Response {
    /// A response with a status and an empty body.
    #[must_use]
    pub const fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// A `200` carrying `body` as `text/plain`.
    #[must_use]
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![(
                "content-type".to_owned(),
                "text/plain; charset=utf-8".to_owned(),
            )],
            body: body.into().into_bytes(),
        }
    }

    /// A `200` carrying `body` as `application/json`.
    ///
    /// The caller supplies already-serialised JSON; this crate does not choose a serialiser for an
    /// application.
    #[must_use]
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: body.into().into_bytes(),
        }
    }

    /// Adds a header.
    ///
    /// # Errors
    ///
    /// Returns [`RouteError::HeaderNotRepresentable`] if the name or value contains a control
    /// character. Refusing here is what stops a handler from splitting a response by returning a
    /// value containing CR or LF — the check is at the only place a header can be added, so there
    /// is no second path that skips it.
    pub fn with_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, RouteError> {
        let name = name.into();
        let value = value.into();

        let offending = |text: &str| text.chars().any(|c| c.is_control());
        if offending(&name) || offending(&value) {
            return Err(RouteError::HeaderNotRepresentable { name });
        }

        self.headers.push((name, value));
        Ok(self)
    }

    /// Replaces the body.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// The status code.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        self.status
    }

    /// The headers, in insertion order.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The body bytes.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

/// Same reasoning as [`Request`]'s: the length is a fact, the content is a disclosure.
impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("headers", &self.headers.len())
            .field("body_len", &self.body.len())
            .finish()
    }
}

/// The future a handler returns.
pub type HandlerFuture = Pin<Box<dyn Future<Output = Response> + Send>>;

/// A transport-neutral request handler.
///
/// Implemented for any `Fn(Request) -> impl Future<Output = Response>`, so an author writes an
/// async function and never names this trait.
pub trait Handler: Send + Sync + 'static {
    /// Handles one request.
    fn call(&self, request: Request) -> HandlerFuture;
}

impl<F, Fut> Handler for F
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    fn call(&self, request: Request) -> HandlerFuture {
        Box::pin(self(request))
    }
}

/// One registered route.
#[derive(Clone)]
pub struct Route {
    pub(crate) method: Method,
    /// The full path, group prefix already applied.
    pub(crate) path: String,
    pub(crate) group: Option<String>,
    pub(crate) handler: Arc<dyn Handler>,
}

impl Route {
    /// The method this route answers.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// The full path, including any group prefix.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The owning group's name, if any.
    #[must_use]
    pub fn group(&self) -> Option<&str> {
        self.group.as_deref()
    }
}

impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("group", &self.group)
            .finish_non_exhaustive()
    }
}

/// A named collection of routes sharing a path prefix.
pub struct RouteGroup {
    name: String,
    prefix: String,
    routes: Vec<Route>,
}

impl RouteGroup {
    /// Creates a group whose routes are prefixed with `prefix`.
    ///
    /// # Errors
    ///
    /// Returns [`RouteError::InvalidPath`] if the prefix is not a valid path segment sequence.
    pub fn new(name: impl Into<String>, prefix: impl Into<String>) -> Result<Self, RouteError> {
        let name = name.into();
        let prefix = prefix.into();
        validate_path(&prefix)?;
        Ok(Self {
            name,
            prefix,
            routes: Vec::new(),
        })
    }

    /// Adds a route to this group.
    ///
    /// # Errors
    ///
    /// Returns [`RouteError::InvalidPath`] if `path` is not a valid route path.
    pub fn route(
        mut self,
        method: Method,
        path: impl Into<String>,
        handler: impl Handler,
    ) -> Result<Self, RouteError> {
        let path = path.into();
        validate_path(&path)?;

        let full = join_paths(&self.prefix, &path);
        self.routes.push(Route {
            method,
            path: full,
            group: Some(self.name.clone()),
            handler: Arc::new(handler),
        });
        Ok(self)
    }

    /// Adds a `GET` route.
    ///
    /// # Errors
    ///
    /// As [`RouteGroup::route`].
    pub fn get(self, path: impl Into<String>, handler: impl Handler) -> Result<Self, RouteError> {
        self.route(Method::Get, path, handler)
    }

    /// Adds a `POST` route.
    ///
    /// # Errors
    ///
    /// As [`RouteGroup::route`].
    pub fn post(self, path: impl Into<String>, handler: impl Handler) -> Result<Self, RouteError> {
        self.route(Method::Post, path, handler)
    }

    /// This group's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn into_routes(self) -> Vec<Route> {
        self.routes
    }
}

/// Joins a group prefix and a route path into one normalised path.
pub(crate) fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    if path == "/" {
        return if prefix.is_empty() {
            "/".to_owned()
        } else {
            prefix.to_owned()
        };
    }
    format!("{prefix}{path}")
}

/// Rejects a path that is empty, unrooted, or carries a control character.
///
/// Validation happens at **registration**, so an invalid pattern is a startup error rather than a
/// route that silently never matches. A route that never matches is the worse failure: it looks
/// registered in every listing and answers nothing.
pub(crate) fn validate_path(path: &str) -> Result<(), RouteError> {
    if path.is_empty() {
        return Err(RouteError::InvalidPath {
            path: path.to_owned(),
            reason: "a path may not be empty",
        });
    }
    if !path.starts_with('/') {
        return Err(RouteError::InvalidPath {
            path: path.to_owned(),
            reason: "a path must begin with `/`",
        });
    }
    if path.chars().any(char::is_control) {
        return Err(RouteError::InvalidPath {
            path: path.to_owned(),
            reason: "a path may not contain a control character",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Method, Response, RouteError, join_paths, validate_path};

    #[test]
    fn methods_render_canonically() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Delete.to_string(), "DELETE");
        assert_eq!(Method::all().len(), 7);
    }

    #[test]
    fn a_group_prefix_and_a_route_path_join_without_a_doubled_slash() {
        assert_eq!(join_paths("/api/v1", "/health"), "/api/v1/health");
        assert_eq!(join_paths("/api/v1/", "/health"), "/api/v1/health");
        assert_eq!(join_paths("", "/health"), "/health");
        // A root route inside a prefixed group is the prefix itself, not `/api/v1/`.
        assert_eq!(join_paths("/api/v1", "/"), "/api/v1");
        assert_eq!(join_paths("", "/"), "/");
    }

    #[test]
    fn an_invalid_path_is_refused_at_registration() {
        assert!(matches!(
            validate_path(""),
            Err(RouteError::InvalidPath { .. })
        ));
        assert!(matches!(
            validate_path("health"),
            Err(RouteError::InvalidPath { .. })
        ));
        assert!(matches!(
            validate_path("/hea\nlth"),
            Err(RouteError::InvalidPath { .. })
        ));

        // POSITIVE CONTROL: a valid path passes, so the refusals above are about the inputs rather
        // than about `validate_path` rejecting everything.
        assert!(validate_path("/health").is_ok());
    }

    #[test]
    fn a_header_carrying_a_control_character_is_refused() {
        // Response splitting: a handler must not be able to inject CR or LF into the response.
        let split = Response::text("ok").with_header("x-test", "value\r\nX-Injected: yes");
        assert!(matches!(
            split,
            Err(RouteError::HeaderNotRepresentable { .. })
        ));

        let named = Response::text("ok").with_header("x-te\nst", "value");
        assert!(matches!(
            named,
            Err(RouteError::HeaderNotRepresentable { .. })
        ));

        // POSITIVE CONTROL: an ordinary header is accepted.
        assert!(Response::text("ok").with_header("x-test", "value").is_ok());
    }

    #[test]
    fn debug_never_prints_a_body() {
        // FR-041: a body is author data of unknown sensitivity.
        let response = Response::text("super-secret-token");
        let rendered = format!("{response:?}");
        assert!(
            !rendered.contains("super-secret-token"),
            "the body reached Debug output: {rendered}"
        );
        // POSITIVE CONTROL: Debug does render, so the absence above is not an empty string.
        assert!(rendered.contains("status"), "{rendered}");
    }
}
