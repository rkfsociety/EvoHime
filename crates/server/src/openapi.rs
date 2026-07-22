use axum::{http::header, response::IntoResponse};

/// Generated OpenAPI route inventory for tooling and typed client generation.
pub async fn document() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        include_str!("../../../docs/openapi.json"),
    )
}
