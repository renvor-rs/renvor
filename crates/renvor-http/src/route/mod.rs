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
pub mod describe;
pub mod inspect;
pub mod registry;
pub mod spec;

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use std::collections::BTreeMap;
use std::sync::Arc;

use renvor_core::{KernelError, TypedStateMap};

use crate::context::RequestContext;

pub use registry::{RouteError, RouteRegistry};
pub use spec::{BodySpec, OperationSpec, ParameterSpec, RequestRejection, ResponseSpec};

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

/// The credentials a request presented — **and nothing else**.
///
/// # Why this exists, and why it is not a header map
///
/// [`Request`] carries no headers on purpose: *"a handler never re-derives client identity from a
/// header, because it never sees the headers that would let it."* That rule is Phase 004's, it is
/// right, and it is unchanged.
///
/// But an authentication flow is defined by a credential the **application** must validate, and a
/// session cookie is not client identity: it is opaque to this crate and meaningful only against a
/// store this crate does not have. Validating one re-derives nothing — the [`ClientIdentity`] an
/// abuse control counts still comes from [`RequestContext`], resolved by the layer that knows the
/// trust configuration.
///
/// So exactly two values cross, and a general `Request::header(name)` deliberately does not exist:
/// `Host`, `Origin`, `Forwarded` and `X-Forwarded-For` stay unreachable from a handler, which is
/// what keeps Phase 004's property exactly as strong as it was.
///
/// Both values are **inert here**. This crate copies them and forms no opinion about either.
///
/// [`ClientIdentity`]: crate::ClientIdentity
/// # `Debug` is hand-written, and `PartialEq` is deliberately absent
///
/// A derived `Debug` prints `cookie: "__Host-rv_session=<live session id>"` — a live credential, in
/// whatever log or panic message formatted it. Every other type in this workspace that holds one
/// refuses that: `renvor_auth::AttemptKeyring` renders `[redacted]`, `AuthenticationService` renders
/// its type name and nothing else, `renvor_config::Secret` exists for it, and `SetCookie` makes the
/// disclosure conspicuous by naming the method `expose_header_value`. This derived it, which was the
/// one place in the diff that broke that discipline.
///
/// `PartialEq` is gone for a second reason: comparing credential material with `==` is a
/// non-constant-time comparison, and offering one on a type that holds a session identifier invites
/// exactly that.
#[derive(Clone, Default)]
pub struct PresentedCredentials {
    cookie: String,
    authorization: String,
}

impl fmt::Debug for PresentedCredentials {
    /// Reports **whether** each credential was presented, never what it was.
    ///
    /// Presence is an operational fact worth having in a diagnostic; the value is the thing the
    /// diagnostic must not carry.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PresentedCredentials")
            .field("cookie", &presence(&self.cookie))
            .field("authorization", &presence(&self.authorization))
            .finish()
    }
}

/// Whether a credential was presented, as the only thing safe to print about it.
const fn presence(value: &str) -> &'static str {
    if value.is_empty() {
        "absent"
    } else {
        "present (redacted)"
    }
}

impl PresentedCredentials {
    /// Builds the pair from the two header values, absent ones as empty strings.
    #[must_use]
    pub fn new(cookie: impl Into<String>, authorization: impl Into<String>) -> Self {
        Self {
            cookie: cookie.into(),
            authorization: authorization.into(),
        }
    }

    /// The raw `Cookie:` header value, or `""`.
    ///
    /// **Unparsed.** Which cookie matters, and what a well-formed one looks like, are the
    /// application's questions; `renvor-auth`'s cookie boundary answers them.
    #[must_use]
    pub fn cookie(&self) -> &str {
        &self.cookie
    }

    /// The raw `Authorization:` header value, or `""`. Unparsed, for the same reason.
    #[must_use]
    pub fn authorization(&self) -> &str {
        &self.authorization
    }
}

/// What a handler receives.
///
/// Deliberately small. Everything a handler needs about *who is asking* lives in
/// [`RequestContext`], which is the value the security layers have already resolved — a handler
/// never re-derives client identity from a header, because it never sees the headers that would
/// let it.
///
/// The one exception is [`PresentedCredentials`], and its documentation explains why a credential
/// the application must validate is not the same thing as an identity the transport resolved.
pub struct Request {
    context: RequestContext,
    body: Vec<u8>,
    query: String,
    path_params: BTreeMap<String, String>,
    state: Arc<TypedStateMap>,
    credentials: PresentedCredentials,
}

impl Request {
    /// Builds a request carrying **no** application state.
    ///
    /// The empty map is not a silent default that could mask a failure: every
    /// [`Request::state`] call against it reports [`KernelError::StateMissing`] **naming the type
    /// asked for**, which is exactly what FR-013 requires. What it avoids is forcing a test that
    /// does not use state to construct a map it will never read.
    ///
    /// The router always calls [`Request::with_state`], so a served request carries the
    /// application's real state.
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
            state: Arc::new(TypedStateMap::new()),
            // EMPTY BY DEFAULT, and that is the fail-closed direction: a request built without
            // credentials presents none, so an authenticated route refuses rather than admitting
            // whatever happened to be in memory.
            credentials: PresentedCredentials::default(),
        }
    }

    /// Attaches the credentials the request presented.
    ///
    /// A builder rather than a parameter of [`Request::new`], so every existing construction site
    /// keeps compiling and keeps presenting nothing.
    #[must_use]
    pub fn with_credentials(mut self, credentials: PresentedCredentials) -> Self {
        self.credentials = credentials;
        self
    }

    /// The credentials the request presented.
    #[must_use]
    pub const fn credentials(&self) -> &PresentedCredentials {
        &self.credentials
    }

    /// Attaches the application's typed state.
    ///
    /// Shared rather than cloned: a `TypedStateMap` holds an application's live resources — a
    /// connection pool, a cache handle — and cloning one per request would either be impossible or
    /// be wrong.
    #[must_use]
    pub fn with_state(mut self, state: Arc<TypedStateMap>) -> Self {
        self.state = state;
        self
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
    ///
    /// The value is **an untrusted string**. It came from the request line, and nothing has
    /// interpreted it. A handler that needs a number parses it and handles the failure.
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params.get(name).map(String::as_str)
    }

    /// Reads a value the application registered in the kernel's typed state (FR-012).
    ///
    /// # This is the whole state bridge, and it deliberately carries no transport type
    ///
    /// The value crossing into a handler is the kernel's own [`TypedStateMap`] entry. `axum` is
    /// not named here, cannot be named here — this module imports none of it — and does not appear
    /// in the returned type or in the error.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::StateMissing`], **naming the type**, when nothing of type `T` is
    /// registered. FR-013 requires this to be an explicit reported failure: it is never a panic,
    /// and never a substituted default. A type-incompatible entry is indistinguishable from an
    /// absent one by construction — [`TypedStateMap`] is keyed by [`core::any::TypeId`], so a
    /// value registered under a different type is simply not found under this one, and reports the
    /// same error rather than being reinterpreted.
    pub fn state<T: core::any::Any + Send + Sync>(&self) -> Result<&T, KernelError> {
        self.state.get::<T>()
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
            // Type NAMES only, never values — `TypedStateMap`'s own `Debug` enforces that, and
            // this reuses it rather than reaching past it.
            .field("state", &self.state)
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
    ///
    /// # Errors
    ///
    /// Returns [`RouteError::StatusNotRepresentable`] for a value outside `100..=599`.
    ///
    /// Validated **here**, at the only place a status is chosen, for the same reason
    /// [`Response::with_header`] validates a header name at the only place one is added. A status
    /// of `0` or `1000` used to construct cleanly and then fail while the response was being
    /// rendered — at which point the handler had already returned, the failure could only be a
    /// bare `500`, and the status the author actually meant was lost.
    pub fn status(status: u16) -> Result<Self, RouteError> {
        // The range HTTP defines. Narrower checks (a closed list of known codes) were rejected:
        // an application legitimately serves a status the framework has not heard of, and a
        // framework refusing `418` would be enforcing its own vocabulary rather than the protocol.
        if !(100..=599).contains(&status) {
            return Err(RouteError::StatusNotRepresentable { status });
        }
        Ok(Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        })
    }

    /// A `200` carrying `body` as `text/plain`.
    ///
    /// Infallible, unlike [`Response::status`]: `200` is a constant this function chose, not a
    /// value a caller supplied, so there is nothing here that could fail.
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

/// A transport-neutral middleware.
///
/// # This is a Renvor seam, not the transport's
///
/// A middleware here receives a [`Request`] and a [`Next`], both declared in this module, and
/// returns a [`Response`]. None of them names `axum`, `tower`, or `http`. That is deliberate: a
/// group's middleware is application logic — an authorisation check, a tenant lookup — and
/// application logic that named a transport type would make the group unusable under any other.
///
/// The transport's own concerns — request identity, host validation, CORS, admission, timeout —
/// are **not** expressed through this trait. They are layers around the whole router, because they
/// must run for requests that match no route at all, which a per-route chain by definition cannot.
///
/// Implemented for any `Fn(Request, Next) -> impl Future<Output = Response>`, so an author writes
/// an async closure and never names this trait.
pub trait Middleware: Send + Sync + 'static {
    /// Handles one request, with the rest of the chain available as `next`.
    ///
    /// Calling [`Next::run`] continues inward. **Not** calling it answers instead of the handler,
    /// which is how an authorisation check refuses.
    fn call(&self, request: Request, next: Next) -> HandlerFuture;
}

impl<F, Fut> Middleware for F
where
    F: Fn(Request, Next) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Response> + Send + 'static,
{
    fn call(&self, request: Request, next: Next) -> HandlerFuture {
        Box::pin(self(request, next))
    }
}

/// The remainder of a middleware chain, ending at the route's handler.
///
/// # An onion, and the shape matters
///
/// Each middleware wraps the ones after it, so a chain `[outer, inner]` runs
/// `outer` → `inner` → handler on the way in and unwinds in the reverse order on the way out. A
/// design that ran them as a flat sequence would give the same entry order and the wrong exit
/// order, and a middleware that measures a duration or releases a resource on the way out would be
/// silently wrong.
pub struct Next {
    chain: Arc<[Arc<dyn Middleware>]>,
    position: usize,
    handler: Arc<dyn Handler>,
}

impl Next {
    /// Builds the chain for one route.
    pub(crate) fn new(chain: Arc<[Arc<dyn Middleware>]>, handler: Arc<dyn Handler>) -> Self {
        Self {
            chain,
            position: 0,
            handler,
        }
    }

    /// Continues inward — to the next middleware, or to the handler when there is none left.
    #[must_use]
    pub fn run(self, request: Request) -> HandlerFuture {
        match self.chain.get(self.position) {
            Some(middleware) => {
                let middleware = Arc::clone(middleware);
                let next = Self {
                    chain: Arc::clone(&self.chain),
                    position: self.position + 1,
                    handler: Arc::clone(&self.handler),
                };
                middleware.call(request, next)
            }
            None => self.handler.call(request),
        }
    }
}

impl fmt::Debug for Next {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Next")
            .field(
                "remaining",
                &(self.chain.len().saturating_sub(self.position)),
            )
            .finish_non_exhaustive()
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
    /// The group middleware wrapping this route, **outermost first**.
    ///
    /// An `Arc<[_]>` rather than a `Vec`: every request builds a [`Next`] over it, and a shared
    /// slice is cloned by reference count rather than by copying the chain per request.
    pub(crate) middleware: Arc<[Arc<dyn Middleware>]>,
    /// What this route declares — the value BOTH runtime validation and the published description
    /// read.
    ///
    /// `Option` because a route may be registered without declarations. Such a route is still
    /// dispatched and still described; it simply declares no inputs. It is never omitted from the
    /// description, because a missing route is the failure the single-source rule exists to
    /// prevent.
    ///
    /// `Arc` because a spec holds schemas and every request reads it: sharing by reference count
    /// costs nothing per request, and cloning one would cost a deep copy of every schema.
    pub(crate) spec: Option<Arc<OperationSpec>>,
}

impl Route {
    /// The method this route answers.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// Runs this route: its group middleware chain, then its handler.
    ///
    /// # Why this is public, and why it is not a way round the chain
    ///
    /// `RouteRegistry` is transport-neutral by design — no type in it names `axum`. Until this
    /// method existed, the only thing that could actually *run* a registered route was the axum
    /// bridge, so "transport-neutral" was true of the types and false of the crate: a second
    /// transport, or a test that wanted to exercise a route table without binding a socket, had no
    /// way in.
    ///
    /// It runs the **middleware first**, in the same onion order a served request takes, and there
    /// is deliberately no accessor that hands out the bare handler. A caller cannot use this to
    /// skip an authorisation middleware, because there is nothing here that skips one.
    ///
    /// What it does not do is the transport's own work — host validation, CORS, admission,
    /// timeouts, request identity. Those are layers around the whole router, because they must run
    /// for requests that match no route at all.
    #[must_use]
    pub fn dispatch(&self, request: Request) -> HandlerFuture {
        Next::new(Arc::clone(&self.middleware), Arc::clone(&self.handler)).run(request)
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

    /// What this route declares, if anything.
    ///
    /// **The same value** `build::router` validates against and `describe::document` publishes.
    #[must_use]
    pub fn spec(&self) -> Option<&OperationSpec> {
        self.spec.as_deref()
    }

    /// How many group middleware wrap this route.
    ///
    /// A count rather than the chain itself: a middleware is author code, and handing it out would
    /// let a caller run it outside the request that gave it its context.
    #[must_use]
    pub fn middleware_count(&self) -> usize {
        self.middleware.len()
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
    middleware: Vec<Arc<dyn Middleware>>,
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
            middleware: Vec::new(),
        })
    }

    /// Adds a middleware that wraps **every** route in this group, and no route outside it.
    ///
    /// Declaration order is outermost-first: the first `layer` call is entered first and exited
    /// last.
    ///
    /// # Why this is applied when a route is added rather than when the group is registered
    ///
    /// Each route captures the chain as it stood when the route was declared. That makes the
    /// group's contents readable top to bottom — a route sees the layers written above it — rather
    /// than depending on what happens to be added afterwards.
    #[must_use]
    pub fn layer(mut self, middleware: impl Middleware) -> Self {
        self.middleware.push(Arc::new(middleware));
        self
    }

    /// Nests `child` inside this group.
    ///
    /// Prefixes compose **left to right** and middleware composes **outermost first**, so a route
    /// in the child runs this group's layers before the child's own, and answers on this group's
    /// prefix followed by the child's.
    ///
    /// # Errors
    ///
    /// Returns [`RouteError::InvalidPath`] if a composed path is not a valid route path.
    pub fn group(mut self, child: Self) -> Result<Self, RouteError> {
        let Self {
            name: child_name,
            prefix: child_prefix,
            routes: child_routes,
            middleware: child_middleware,
        } = child;
        let _ = child_name;
        let _ = child_prefix;
        let _ = child_middleware;

        for route in child_routes {
            let composed = join_paths(&self.prefix, &route.path);
            validate_path(&composed)?;

            // This group's layers go OUTSIDE the child's, which the child already baked into each
            // of its routes. Prepending rather than appending is what makes "outer first" true.
            let mut chain: Vec<Arc<dyn Middleware>> = self.middleware.clone();
            chain.extend(route.middleware.iter().map(Arc::clone));

            self.routes.push(Route {
                method: route.method,
                path: composed,
                group: route.group,
                handler: route.handler,
                middleware: chain.into(),
                spec: None,
            });
        }
        Ok(self)
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
            middleware: self.middleware.clone().into(),
            spec: None,
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
    use super::{Method, PresentedCredentials, Response, RouteError, join_paths, validate_path};

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
    fn a_request_presents_no_credential_by_default() {
        // FAIL-CLOSED. `Request::new` attaches `PresentedCredentials::default()`, so a request
        // built without credentials presents none and an authenticated route refuses rather than
        // admitting whatever happened to be in memory.
        let none = PresentedCredentials::default();
        assert_eq!(none.cookie(), "");
        assert_eq!(none.authorization(), "");

        // POSITIVE CONTROL: the accessors DO return what was supplied, so the emptiness above is
        // about the default rather than about accessors that always answer "".
        let carrying = PresentedCredentials::new("__Host-rv_session=abc", "Bearer token");
        assert_eq!(carrying.cookie(), "__Host-rv_session=abc");
        assert_eq!(carrying.authorization(), "Bearer token");
    }

    #[test]
    fn the_credential_pair_carries_two_values_and_offers_no_way_to_ask_for_a_third() {
        // The WHOLE surface, enumerated. `PresentedCredentials` has exactly two accessors and no
        // `get(name)`, so `Host`, `Origin`, `Forwarded` and `X-Forwarded-For` remain unreachable
        // from a handler — which is the Phase 004 property this addition had to leave intact.
        //
        // This is a structural claim and the assertion below is only its shadow: what enforces it
        // is that no other method exists. The test is here so that adding one is a deliberate act
        // next to this comment rather than an unremarked line in an impl block.
        let credentials = PresentedCredentials::new("__Host-rv_session=abc", "Bearer tok");
        let rendered = format!("{credentials:?}");
        assert!(rendered.contains("cookie"));
        assert!(rendered.contains("authorization"));
        for absent in ["host", "origin", "forwarded", "x-forwarded-for", "referer"] {
            assert!(
                !rendered.contains(absent),
                "the credential pair grew a field it should not have"
            );
        }

        // AND NEITHER VALUE IS RENDERED. A derived `Debug` printed both in full; a security review
        // found it, and this is what stops it coming back.
        assert!(!rendered.contains("abc"), "the cookie value was printed");
        assert!(!rendered.contains("tok"), "the bearer token was printed");
        assert!(rendered.contains("present (redacted)"));

        // POSITIVE CONTROL: an absent credential is reported as absent rather than as redacted, so
        // the marker above is about the value rather than about a constant string.
        let empty = PresentedCredentials::default();
        assert!(format!("{empty:?}").contains("absent"));
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
