//! The example domain: an item with a name.
//!
//! The type, its JSON shapes, and the HTTP handlers over the repository. The repository itself
//! is in `src/persistence.rs`; nothing in this file names a database.

use renvor::ApiErrorCode;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Item {
    /// Its identifier, assigned by the database.
    pub id: i64,
    /// Its name.
    pub name: String,
}

/// The body of `POST /items` and `PUT /items/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct SaveItem {
    /// The name. 1 to 200 characters, matching the column.
    #[schemars(length(min = 1, max = 200))]
    pub name: String,
}

impl SaveItem {
    /// Validates the body against the column's bound.
    pub fn validate(&self) -> Result<(), ApiErrorCode> {
        let length = self.name.chars().count();
        if length == 0 || length > 200 {
            return Err(ApiErrorCode::ValidationFailed);
        }
        Ok(())
    }
}

/// The HTTP handlers.
pub mod routes {
    use super::SaveItem;
    use crate::routes::{json, problem, services};
    use renvor::{ApiErrorCode, Request, Response};

    fn id_from(request: &Request) -> Option<i64> {
        request.path_param("id")?.parse().ok()
    }

    fn body(request: &Request) -> Result<SaveItem, Response> {
        let input: SaveItem = serde_json::from_slice(request.body())
            .map_err(|_| problem(request, ApiErrorCode::MalformedBody))?;
        input.validate().map_err(|code| problem(request, code))?;
        Ok(input)
    }

    /// `GET /items`.
    pub async fn list(request: Request) -> Response {
        let services = match services(&request) {
            Ok(services) => services,
            Err(response) => return response,
        };
        match crate::persistence::list(services).await {
            Ok(items) => json(200, &items),
            Err(code) => problem(&request, code),
        }
    }

    /// `GET /items/{id}`.
    pub async fn get(request: Request) -> Response {
        let Some(id) = id_from(&request) else {
            return problem(&request, ApiErrorCode::ValidationFailed);
        };
        let services = match services(&request) {
            Ok(services) => services,
            Err(response) => return response,
        };
        match crate::persistence::get(services, id).await {
            Ok(Some(item)) => json(200, &item),
            Ok(None) => problem(&request, ApiErrorCode::ResourceNotFound),
            Err(code) => problem(&request, code),
        }
    }

    /// `POST /items`.
    pub async fn create(request: Request) -> Response {
        let services = match services(&request) {
            Ok(services) => services,
            Err(response) => return response,
        };
        let input = match body(&request) {
            Ok(input) => input,
            Err(response) => return response,
        };
        match crate::persistence::create(services, input).await {
            Ok(item) => json(201, &item),
            Err(code) => problem(&request, code),
        }
    }

    /// `PUT /items/{id}`.
    pub async fn replace(request: Request) -> Response {
        let Some(id) = id_from(&request) else {
            return problem(&request, ApiErrorCode::ValidationFailed);
        };
        let services = match services(&request) {
            Ok(services) => services,
            Err(response) => return response,
        };
        let input = match body(&request) {
            Ok(input) => input,
            Err(response) => return response,
        };
        match crate::persistence::replace(services, id, input).await {
            Ok(Some(item)) => json(200, &item),
            Ok(None) => problem(&request, ApiErrorCode::ResourceNotFound),
            Err(code) => problem(&request, code),
        }
    }

    /// `DELETE /items/{id}`.
    pub async fn delete(request: Request) -> Response {
        let Some(id) = id_from(&request) else {
            return problem(&request, ApiErrorCode::ValidationFailed);
        };
        let services = match services(&request) {
            Ok(services) => services,
            Err(response) => return response,
        };
        match crate::persistence::delete(services, id).await {
            Ok(true) => Response::status(204).expect("204 is valid"),
            Ok(false) => problem(&request, ApiErrorCode::ResourceNotFound),
            Err(code) => problem(&request, code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SaveItem;

    #[test]
    fn a_name_is_bounded_like_its_column() {
        assert!(
            SaveItem {
                name: "a".repeat(200)
            }
            .validate()
            .is_ok()
        );
        assert!(
            SaveItem {
                name: "a".repeat(201)
            }
            .validate()
            .is_err()
        );
        assert!(
            SaveItem {
                name: String::new()
            }
            .validate()
            .is_err()
        );
    }
}
