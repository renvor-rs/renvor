//! `/healthz` and `/readyz` as a `RouteGroup`, behind the `http` feature (FR-073).
//!
//! The group reads a **cloned** `HealthState` (B-2: the state is `Arc`-backed and `Clone`, so a
//! provider can hold it and a probe handler can read it). Liveness and readiness are separate
//! routes with separate documents (C-O8): `200` when the answer is yes, `503` when it is no, the
//! JSON document either way. An application registers the group where it wants it; nothing here
//! is registered by default, because a route that exists is a route that answers.

use renvor_core::HealthState;
use renvor_http::{Request, Response, RouteError, RouteGroup};

use crate::health::{is_alive, is_ready, liveness_document, readiness_document};

/// The liveness route's path.
pub const HEALTHZ: &str = "/healthz";
/// The readiness route's path.
pub const READYZ: &str = "/readyz";
/// The group's name.
pub const GROUP_NAME: &str = "health";

/// The response for a document: `200` with the JSON body when `ok`, `503` with the same body
/// otherwise.
fn document(ok: bool, body: String) -> Response {
    if ok {
        return Response::json(body);
    }
    match Response::status(503)
        .and_then(|response| response.with_header("content-type", "application/json"))
    {
        Ok(response) => response.with_body(body),
        // A fixed status and a fixed header name and value cannot be refused; if they ever
        // were, a `503` with no body still says "not healthy", which is the safe answer.
        Err(_) => Response::status(503).unwrap_or_else(|_| Response::json(body)),
    }
}

/// The two health routes over `health`, at the root (the group prefix is `/`, which the router
/// joins to `/healthz` and `/readyz` without doubling the slash).
///
/// # Errors
///
/// [`RouteError`] only if the fixed paths or name were refused, which the route rules do not do.
pub fn health_routes(health: HealthState) -> Result<RouteGroup, RouteError> {
    let live = health.clone();
    let ready = health;
    RouteGroup::new(GROUP_NAME, "/")?
        .get(HEALTHZ, move |_request: Request| {
            let health = live.clone();
            async move { document(is_alive(&health), liveness_document(&health)) }
        })?
        .get(READYZ, move |_request: Request| {
            let health = ready.clone();
            async move { document(is_ready(&health), readiness_document(&health)) }
        })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use renvor_core::HealthState;
    use renvor_core::cancel::CancelScope;
    use renvor_core::health::{Liveness, Readiness, ReadinessContributor};
    use renvor_core::observe::{OsEntropy, RunIdentifier};
    use renvor_http::{ClientIdentity, Request, RequestContext, RequestId, RouteRegistry};

    use super::{HEALTHZ, READYZ, health_routes};

    #[derive(Debug)]
    struct Fixed(Readiness);

    impl ReadinessContributor for Fixed {
        fn name(&self) -> &str {
            "fixed"
        }
        fn readiness(&self) -> Readiness {
            self.0
        }
    }

    fn request() -> Request {
        let context = RequestContext::new(
            RunIdentifier::generate(&OsEntropy::new()).unwrap(),
            RequestId::from_entropy([1; 8]),
            ClientIdentity::DirectPeer(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            "localhost",
            CancelScope::root(),
        );
        Request::new(context, Vec::new(), String::new(), BTreeMap::new())
    }

    async fn call(health: &HealthState, path: &str) -> (u16, serde_json::Value) {
        let mut registry = RouteRegistry::new();
        registry
            .group(health_routes(health.clone()).unwrap())
            .unwrap();
        let route = registry
            .routes()
            .iter()
            .find(|route| route.path() == path)
            .expect("the route is registered");
        let response = route.dispatch(request()).await;
        let body: serde_json::Value =
            serde_json::from_slice(response.body()).expect("a JSON document");
        assert!(
            response
                .headers()
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("content-type")
                    && value.starts_with("application/json")),
            "the document is not served as JSON"
        );
        (response.status_code(), body)
    }

    #[tokio::test]
    async fn the_two_routes_answer_independently_with_the_documents() {
        let health = HealthState::new();
        health.register(Arc::new(Fixed(Readiness::NotReady)));
        let (status, body) = call(&health, HEALTHZ).await;
        assert_eq!(status, 200);
        assert_eq!(body["status"], "alive");
        let (status, body) = call(&health, READYZ).await;
        assert_eq!(status, 503, "not ready must be 503");
        assert_eq!(body["status"], "not_ready");
        assert_eq!(body["contributors"][0]["name"], "fixed");

        health.set_liveness(Liveness::Dead);
        let (status, body) = call(&health, HEALTHZ).await;
        assert_eq!(status, 503);
        assert_eq!(body["status"], "dead");
    }
}
