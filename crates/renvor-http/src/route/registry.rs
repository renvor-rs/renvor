//! The route registry: **the single authoritative source** of route metadata.
//!
//! # There is nowhere for a second manifest to live
//!
//! Contract C-9 prohibits a second route list that can drift from the router. That prohibition is
//! structural here rather than advisory:
//!
//! ```text
//!                  RouteRegistry
//!                 (one value)
//!                 ╱          ╲
//!        build::router()   inspect::render()
//! ```
//!
//! Both functions take `&RouteRegistry`. Neither has access to any other source, so a route cannot
//! reach dispatch without reaching inspection, and cannot reach inspection without reaching
//! dispatch. Making them agree is not a discipline anyone has to keep — there is only one value.
//!
//! # Why a duplicate is an error rather than a replacement
//!
//! Silently replacing makes the winner depend on registration order that nobody wrote down, and
//! the loser is invisible: the route is *listed*, it just never runs. `TypedStateMap` refuses a
//! duplicate registration for the same reason, and this is the same rule applied to routes.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use super::{Handler, Method, Route, RouteGroup, validate_path};
use crate::limits::MAX_ROUTES;

/// A failure while building a route table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RouteError {
    /// The same method and path were registered twice.
    Duplicate {
        /// The method registered twice.
        method: Method,
        /// The path registered twice.
        path: String,
    },
    /// More routes were registered than the documented ceiling permits.
    CeilingExceeded {
        /// The documented ceiling.
        limit: usize,
    },
    /// A path was not a valid route pattern.
    InvalidPath {
        /// The offending path.
        path: String,
        /// Why it was refused.
        reason: &'static str,
    },
    /// A response header name or value could not be represented safely.
    HeaderNotRepresentable {
        /// The offending header name.
        name: String,
    },
    /// The method is answered by the transport itself and cannot carry an application route.
    MethodReservedByTransport {
        /// The method that cannot be registered.
        method: Method,
        /// Why the transport answers it.
        reason: &'static str,
    },
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate { method, path } => write!(
                f,
                "`{method} {path}` is already registered; a duplicate route is refused rather than \
                 replacing the first, because a silent winner depends on registration order"
            ),
            Self::CeilingExceeded { limit } => write!(
                f,
                "the route ceiling of {limit} was exceeded; raise it deliberately or register \
                 fewer routes"
            ),
            Self::InvalidPath { path, reason } => {
                write!(f, "`{path}` is not a valid route path: {reason}")
            }
            Self::HeaderNotRepresentable { name } => write!(
                f,
                "the header `{name}` could not be represented: a name or value containing a \
                 control character is refused, because it would allow a response to be split"
            ),
            Self::MethodReservedByTransport { method, reason } => write!(
                f,
                "`{method}` cannot be registered as an application route: {reason}. Registering \
                 it would produce a route that appears in every listing and answers nothing, \
                 which is the failure route validation exists to prevent"
            ),
        }
    }
}

impl core::error::Error for RouteError {}

/// The single authoritative collection of routes.
///
/// Build it with [`RouteRegistry::new`] and the registration methods, then hand the **same value**
/// to router construction and to inspection.
#[derive(Default)]
pub struct RouteRegistry {
    routes: Vec<Route>,
    seen: BTreeSet<(Method, String)>,
}

impl RouteRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            routes: Vec::new(),
            seen: BTreeSet::new(),
        }
    }

    /// Registers one route.
    ///
    /// # Errors
    ///
    /// - [`RouteError::InvalidPath`] if the path is not a valid pattern.
    /// - [`RouteError::Duplicate`] if this method and path are already registered. **The existing
    ///   route is left untouched.**
    /// - [`RouteError::CeilingExceeded`] if the registry already holds [`MAX_ROUTES`] routes.
    pub fn route(
        &mut self,
        method: Method,
        path: impl Into<String>,
        handler: impl Handler,
    ) -> Result<&mut Self, RouteError> {
        let path = path.into();
        validate_path(&path)?;
        self.push(Route {
            method,
            path,
            group: None,
            handler: std::sync::Arc::new(handler),
        })?;
        Ok(self)
    }

    /// Registers a `GET` route.
    ///
    /// # Errors
    ///
    /// As [`RouteRegistry::route`].
    pub fn get(
        &mut self,
        path: impl Into<String>,
        handler: impl Handler,
    ) -> Result<&mut Self, RouteError> {
        self.route(Method::Get, path, handler)
    }

    /// Registers a `POST` route.
    ///
    /// # Errors
    ///
    /// As [`RouteRegistry::route`].
    pub fn post(
        &mut self,
        path: impl Into<String>,
        handler: impl Handler,
    ) -> Result<&mut Self, RouteError> {
        self.route(Method::Post, path, handler)
    }

    /// Registers every route in `group`.
    ///
    /// # Errors
    ///
    /// As [`RouteRegistry::route`], applied to each of the group's routes. A group that collides
    /// part-way through leaves the routes registered before the collision in place and reports the
    /// collision — the same behaviour as registering them one at a time, so grouping changes no
    /// semantics.
    pub fn group(&mut self, group: RouteGroup) -> Result<&mut Self, RouteError> {
        for route in group.into_routes() {
            self.push(route)?;
        }
        Ok(self)
    }

    fn push(&mut self, route: Route) -> Result<(), RouteError> {
        // METHOD FIRST, and this is a refusal rather than a shadow.
        //
        // The selected CORS middleware answers **every** `OPTIONS` request as a preflight — its
        // dispatch is `if parts.method == Method::OPTIONS`, with no check for
        // `Access-Control-Request-Method`. An application route on `OPTIONS` is therefore
        // unreachable: verified, it returned an empty `200` while the handler never ran.
        //
        // Contract C-9 says a route that never matches is the worse failure, because *"it looks
        // registered in every listing and answers nothing"*. So this is refused at registration,
        // where an author sees it, rather than silently shadowed at run time.
        if route.method == Method::Options {
            return Err(RouteError::MethodReservedByTransport {
                method: route.method,
                reason: "the CORS layer answers every OPTIONS request as a preflight",
            });
        }

        // Ceiling first: a registry at its limit refuses regardless of what is being added, and
        // reporting "duplicate" for a route that would have been refused anyway would name the
        // wrong cause.
        if self.routes.len() >= MAX_ROUTES {
            return Err(RouteError::CeilingExceeded { limit: MAX_ROUTES });
        }

        let key = (route.method, route.path.clone());
        if self.seen.contains(&key) {
            return Err(RouteError::Duplicate {
                method: route.method,
                path: route.path,
            });
        }

        self.seen.insert(key);
        self.routes.push(route);
        Ok(())
    }

    /// Every registered route, in registration order.
    #[must_use]
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// How many routes are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Whether the registry holds no routes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// The methods declared for `path`, sorted canonically.
    ///
    /// This is what produces the `Allow` header on a `405`, and it reads the same routes dispatch
    /// reads. An `Allow` header derived from a second list could name a method that does not
    /// answer.
    #[must_use]
    pub fn methods_for(&self, path: &str) -> Vec<Method> {
        let declared: BTreeSet<Method> = self
            .routes
            .iter()
            .filter(|route| route.path == path)
            .map(|route| route.method)
            .collect();

        Method::all()
            .into_iter()
            .filter(|method| declared.contains(method))
            .collect()
    }

    /// Routes grouped by path, for router construction.
    pub(crate) fn by_path(&self) -> BTreeMap<&str, Vec<&Route>> {
        let mut grouped: BTreeMap<&str, Vec<&Route>> = BTreeMap::new();
        for route in &self.routes {
            grouped.entry(route.path.as_str()).or_default().push(route);
        }
        grouped
    }
}

impl fmt::Debug for RouteRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RouteRegistry")
            .field("routes", &self.routes.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{Method, RouteError, RouteRegistry};
    use crate::limits::MAX_ROUTES;
    use crate::route::{Request, Response, RouteGroup};

    async fn ok(_: Request) -> Response {
        Response::text("ok")
    }

    async fn other(_: Request) -> Response {
        Response::text("other")
    }

    #[test]
    fn a_duplicate_is_refused_and_the_first_route_survives() {
        let mut registry = RouteRegistry::new();
        registry.get("/health", ok).expect("first registration");

        let error = registry
            .get("/health", other)
            .expect_err("a duplicate must be refused");

        assert!(matches!(
            error,
            RouteError::Duplicate {
                method: Method::Get,
                ..
            }
        ));

        // The refusal is not enough on its own: the requirement is that the ORIGINAL survives
        // untouched, which a "refuse then replace" bug would also fail.
        assert_eq!(registry.len(), 1, "the duplicate was stored anyway");

        // POSITIVE CONTROL: a different method on the same path is NOT a duplicate.
        registry
            .post("/health", other)
            .expect("a different method is a different route");
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn the_route_ceiling_is_enforced_at_its_exact_boundary() {
        let mut registry = RouteRegistry::new();
        for index in 0..MAX_ROUTES {
            registry
                .get(format!("/r{index}"), ok)
                .unwrap_or_else(|error| panic!("route {index} was refused: {error}"));
        }
        assert_eq!(
            registry.len(),
            MAX_ROUTES,
            "the boundary route was accepted"
        );

        let error = registry
            .get("/one-too-many", ok)
            .expect_err("the route past the ceiling must be refused");
        assert!(matches!(
            error,
            RouteError::CeilingExceeded { limit } if limit == MAX_ROUTES
        ));
    }

    #[test]
    fn a_group_prefixes_every_route_it_holds() {
        let group = RouteGroup::new("api-v1", "/api/v1")
            .expect("a valid prefix")
            .get("/health", ok)
            .expect("a valid route")
            .post("/items", other)
            .expect("a valid route");

        let mut registry = RouteRegistry::new();
        registry.group(group).expect("the group registers");

        let paths: Vec<&str> = registry.routes().iter().map(super::Route::path).collect();
        assert_eq!(paths, vec!["/api/v1/health", "/api/v1/items"]);
        assert_eq!(registry.routes()[0].group(), Some("api-v1"));
    }

    #[test]
    fn an_options_route_is_refused_rather_than_silently_shadowed() {
        // Verified before this refusal existed: registering `OPTIONS /thing` and then requesting
        // it returned `200` with an EMPTY body — the CORS layer's preflight answer — while the
        // handler never ran. A listing showed the route; nothing served it.
        let mut registry = RouteRegistry::new();
        let error = registry
            .route(Method::Options, "/thing", ok)
            .expect_err("an OPTIONS route must be refused");

        assert!(matches!(
            error,
            RouteError::MethodReservedByTransport {
                method: Method::Options,
                ..
            }
        ));
        assert_eq!(registry.len(), 0, "the refused route was stored anyway");

        // POSITIVE CONTROL: every other method on the same path registers, so the refusal is about
        // the method rather than about the path or about registration failing generally.
        for method in [
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Patch,
            Method::Delete,
            Method::Head,
        ] {
            registry
                .route(method, "/thing", ok)
                .unwrap_or_else(|error| panic!("`{method}` was refused: {error}"));
        }
        assert_eq!(registry.len(), 6);
    }

    #[test]
    fn methods_for_reads_the_routes_dispatch_reads() {
        let mut registry = RouteRegistry::new();
        registry.post("/items", ok).expect("registers");
        registry.get("/items", other).expect("registers");
        registry.get("/elsewhere", ok).expect("registers");

        // Canonical order, not registration order — so the `Allow` header is deterministic.
        assert_eq!(
            registry.methods_for("/items"),
            vec![Method::Get, Method::Post]
        );

        // POSITIVE CONTROL: a path with no routes yields none, so the list above is a fact about
        // the registry rather than about the function always returning something.
        assert!(registry.methods_for("/absent").is_empty());
    }
}
