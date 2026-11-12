use std::sync::{Arc, Mutex};

use lexio_rent::{Error, Lexio, Params, OPERATION_COUNT};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn client(server: &MockServer) -> Lexio {
    Lexio::builder().api_key("lck_test").base_url(server.uri()).max_retries(2).build().unwrap()
}

#[tokio::test]
async fn bearer_auth_and_user_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/organization"))
        .and(header("authorization", "Bearer lck_test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"name": "Demo"}})))
        .expect(1)
        .mount(&server)
        .await;
    let org = client(&server).organization().get().await.unwrap();
    assert_eq!(org["data"]["name"], "Demo");
    let req = &server.received_requests().await.unwrap()[0];
    assert!(req.headers.get("user-agent").unwrap().to_str().unwrap().starts_with("lexio-rent-rust/"));
    assert!(req.headers.get("x-api-key").is_none());
}

#[tokio::test]
async fn missing_key_is_an_error() {
    std::env::remove_var("LEXIO_API_KEY");
    assert!(matches!(Lexio::builder().build(), Err(Error::MissingApiKey)));
}

#[tokio::test]
async fn query_and_path_parameters() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/vehicles"))
        .and(query_param("isArchived", "false"))
        .and(query_param("categoryId", "abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [], "nextCursor": null})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rentals/r%201/payments/p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"id": "p2"}})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    c.vehicles().list(&Params::new().set("isArchived", false).set("categoryId", "abc")).await.unwrap();
    let p = c.rentals().get_payment("r 1", "p2").await.unwrap();
    assert_eq!(p["data"]["id"], "p2");
}

struct FailOnce {
    keys: Arc<Mutex<Vec<String>>>,
}

impl Respond for FailOnce {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let mut keys = self.keys.lock().unwrap();
        keys.push(req.headers.get("idempotency-key").unwrap().to_str().unwrap().to_string());
        if keys.len() == 1 {
            ResponseTemplate::new(503).insert_header("Retry-After", "0").set_body_json(json!({"code": "INTERNAL"}))
        } else {
            ResponseTemplate::new(201).set_body_json(json!({"data": {"id": "r1"}}))
        }
    }
}

#[tokio::test]
async fn idempotency_key_generated_and_reused_on_retry() {
    let server = MockServer::start().await;
    let keys = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/rentals"))
        .and(body_json(json!({"quoteId": "q1"})))
        .respond_with(FailOnce { keys: keys.clone() })
        .mount(&server)
        .await;
    let out = client(&server).rentals().create(&json!({"quoteId": "q1"}), None).await.unwrap();
    assert_eq!(out["data"]["id"], "r1");
    let keys = keys.lock().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[0].len(), 36);
}

#[tokio::test]
async fn explicit_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/rentals"))
        .and(header("idempotency-key", "my-key"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {}})))
        .expect(1)
        .mount(&server)
        .await;
    client(&server).rentals().create(&json!({"quoteId": "q1"}), Some("my-key")).await.unwrap();
}

#[tokio::test]
async fn no_retry_on_non_idempotent_write() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/whatever"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"code": "INTERNAL"})))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server)
        .request(lexio_rent::Method::POST, "/whatever", None, Some(&json!({})), None, false)
        .await
        .unwrap_err();
    assert_eq!(err.status(), Some(503));
}

#[tokio::test]
async fn no_content_returns_null() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/vehicles/v1"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let out = client(&server).vehicles().delete("v1", None).await.unwrap();
    assert!(out.is_null());
}

#[tokio::test]
async fn problem_json_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/vehicles/missing"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"code": "NOT_FOUND", "detail": "x", "requestId": "rq1"})),
        )
        .mount(&server)
        .await;
    let err = client(&server).vehicles().get("missing").await.unwrap_err();
    assert!(err.is_not_found());
    assert_eq!(err.code(), Some("NOT_FOUND"));
    match err {
        Error::Api { request_id, .. } => assert_eq!(request_id.as_deref(), Some("rq1")),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn rate_limit_exhausted() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/vehicles"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0").set_body_json(json!({"code": "RATE_LIMITED"})))
        .expect(3)
        .mount(&server)
        .await;
    let err = client(&server).vehicles().list(&Params::new()).await.unwrap_err();
    assert!(err.is_rate_limited());
    match err {
        Error::Api { retry_after, .. } => assert_eq!(retry_after, Some(0)),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn list_all_follows_cursor_and_stops_on_short_page() {
    let server = MockServer::start().await;
    let full: Vec<_> = (0..100).map(|i| json!({"id": i})).collect();
    Mock::given(method("GET"))
        .and(path("/events"))
        .and(query_param("cursor", "c2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": 100}], "nextCursor": "c3"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/events"))
        .and(query_param("start", "EARLIEST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": full, "nextCursor": "c2"})))
        .mount(&server)
        .await;
    let c = client(&server);
    let all = c.events().list_all(&Params::new().set("start", "EARLIEST"), None).await.unwrap();
    assert_eq!(all.len(), 101);
    let capped = c.events().list_all(&Params::new().set("start", "EARLIEST"), Some(5)).await.unwrap();
    assert_eq!(capped.len(), 5);
}

#[test]
fn every_operation_is_generated() {
    assert_eq!(OPERATION_COUNT, 93);
}
