//! Integration tests for facet-axum.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::{get, post},
};
use facet::Facet;
use facet_axum::{Form, Json, Query};
use tower::ServiceExt;

#[derive(Debug, Facet, PartialEq)]
struct User {
    name: String,
    age: u64,
}

#[derive(Debug, Facet, PartialEq)]
struct SearchParams {
    query: String,
    page: u64,
}

async fn echo_json(Json(user): Json<User>) -> Json<User> {
    Json(user)
}

async fn echo_form(Form(user): Form<User>) -> String {
    format!("name={}, age={}", user.name, user.age)
}

async fn search(Query(params): Query<SearchParams>) -> String {
    format!("query={}, page={}", params.query, params.page)
}

fn app() -> Router {
    Router::new()
        .route("/json", post(echo_json))
        .route("/form", post(echo_form))
        .route("/search", get(search))
}

#[tokio::test]
async fn test_json_extractor() {
    let app = app();

    let request = Request::builder()
        .method("POST")
        .uri("/json")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name": "Alice", "age": 30}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert!(body_str.contains("Alice"));
    assert!(body_str.contains("30"));
}

#[tokio::test]
async fn test_json_missing_content_type() {
    let app = app();

    let request = Request::builder()
        .method("POST")
        .uri("/json")
        .body(Body::from(r#"{"name": "Alice", "age": 30}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn test_form_extractor() {
    let app = app();

    let request = Request::builder()
        .method("POST")
        .uri("/form")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("name=Bob&age=25"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(body_str, "name=Bob, age=25");
}

#[tokio::test]
async fn test_query_extractor() {
    let app = app();

    let request = Request::builder()
        .method("GET")
        .uri("/search?query=rust&page=1")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(body_str, "query=rust, page=1");
}
