//! HTTP client: authentication, retries, idempotency, pagination, errors.

use std::time::Duration;

use reqwest::{header, Method, StatusCode};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::params::Params;

/// Production base URL of the Lexio Rent API v1.
pub const DEFAULT_BASE_URL: &str = "https://api.lexio.rent/api/external/v1";

const USER_AGENT: &str = concat!(
    "lexio-rent-rust/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/lexio-rent-git/lexio-rust)"
);
const RETRY_STATUSES: [u16; 5] = [429, 500, 502, 503, 504];
const MAX_RETRY_AFTER: u64 = 60;
const PAGE_SIZE: usize = 100;

/// Client for the Lexio Rent API v1.
///
/// Cheap to clone: clones share the same connection pool.
#[derive(Debug, Clone)]
pub struct Lexio {
    pub(crate) http: reqwest::Client,
    api_key: String,
    base_url: String,
    max_retries: u32,
}

/// Builder for [`Lexio`].
#[derive(Debug, Default)]
pub struct LexioBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    max_retries: Option<u32>,
    timeout: Option<Duration>,
    http: Option<reqwest::Client>,
}

impl LexioBuilder {
    /// API key (`lck_...`). Defaults to `LEXIO_API_KEY`.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Base URL. Defaults to `LEXIO_BASE_URL`, then production.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Retries on 429 and 5xx, for reads and idempotent writes (default 2).
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    /// Request timeout (default 30 s). Ignored when a custom `reqwest::Client` is given.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Use your own `reqwest::Client` (proxy, custom TLS...).
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http = Some(client);
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Lexio> {
        let api_key = match self.api_key {
            Some(k) if !k.is_empty() => k,
            _ => std::env::var("LEXIO_API_KEY").ok().filter(|k| !k.is_empty()).ok_or(Error::MissingApiKey)?,
        };
        let base_url = self
            .base_url
            .or_else(|| std::env::var("LEXIO_BASE_URL").ok().filter(|u| !u.is_empty()))
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let http = match self.http {
            Some(c) => c,
            None => reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(self.timeout.unwrap_or(Duration::from_secs(30)))
                .build()?,
        };
        Ok(Lexio { http, api_key, base_url, max_retries: self.max_retries.unwrap_or(2) })
    }
}

impl Lexio {
    /// Client with an explicit API key and default settings.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Self::builder().api_key(api_key).build()
    }

    /// Client configured from `LEXIO_API_KEY` (and optionally `LEXIO_BASE_URL`).
    pub fn from_env() -> Result<Self> {
        Self::builder().build()
    }

    /// Start a [`LexioBuilder`].
    pub fn builder() -> LexioBuilder {
        LexioBuilder::default()
    }

    /// Base URL in use.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Send any request to the API. `path` is relative to the base URL (`/vehicles`).
    ///
    /// Returns `Value::Null` on `204 No Content`. When `idempotency_required` is
    /// true and no key is given, a UUID v4 key is generated; the same key is
    /// reused on every retry, so a write is never applied twice.
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        params: Option<&Params>,
        body: Option<&Value>,
        idempotency_key: Option<&str>,
        idempotency_required: bool,
    ) -> Result<Value> {
        let key = match idempotency_key {
            Some(k) => Some(k.to_string()),
            None if idempotency_required => Some(uuid::Uuid::new_v4().to_string()),
            None => None,
        };
        let retryable = method == Method::GET || key.is_some();
        let url = format!("{}{}", self.base_url, path);
        let mut attempt: u32 = 0;
        loop {
            let mut req = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&self.api_key)
                .header(header::ACCEPT, "application/json")
                .header(header::USER_AGENT, USER_AGENT);
            if let Some(p) = params {
                req = req.query(p.pairs());
            }
            if let Some(b) = body {
                req = req.json(b);
            }
            if let Some(k) = &key {
                req = req.header("Idempotency-Key", k);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if status.is_success() {
                if status == StatusCode::NO_CONTENT {
                    return Ok(Value::Null);
                }
                let bytes = resp.bytes().await?;
                if bytes.is_empty() {
                    return Ok(Value::Null);
                }
                return serde_json::from_slice(&bytes).map_err(|e| Error::Api {
                    status: status.as_u16(),
                    code: Some("INVALID_JSON".into()),
                    detail: Some(e.to_string()),
                    request_id: None,
                    retry_after: None,
                    problem: Value::Null,
                });
            }
            let retry_after = resp
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            if retryable && attempt < self.max_retries && RETRY_STATUSES.contains(&status.as_u16()) {
                let wait = match retry_after {
                    Some(s) => Duration::from_secs(s.min(MAX_RETRY_AFTER)),
                    None => Duration::from_millis(500 * 2u64.pow(attempt)),
                };
                tokio::time::sleep(wait).await;
                attempt += 1;
                continue;
            }
            let header_request_id =
                resp.headers().get("x-request-id").and_then(|v| v.to_str().ok()).map(str::to_string);
            let problem: Value = resp.json().await.unwrap_or(Value::Null);
            let text = |k: &str| problem.get(k).and_then(Value::as_str).map(str::to_string);
            return Err(Error::Api {
                status: status.as_u16(),
                code: text("code"),
                detail: text("detail"),
                request_id: text("requestId").or(header_request_id),
                retry_after,
                problem: problem.clone(),
            });
        }
    }

    /// Read every item of a list endpoint, following `nextCursor`.
    ///
    /// Stops on a short page (the event feed always returns a cursor) or after
    /// `max_items` items.
    pub async fn list_all(&self, path: &str, params: &Params, max_items: Option<usize>) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut p = params.clone();
            if p.get("limit").is_none() {
                p = p.set("limit", PAGE_SIZE);
            }
            if let Some(c) = &cursor {
                p = p.set("cursor", c);
            }
            let page = self.request(Method::GET, path, Some(&p), None, None, false).await?;
            let data = page.get("data").and_then(Value::as_array).cloned().unwrap_or_default();
            let page_len = data.len();
            let limit: usize = p.get("limit").and_then(|l| l.parse().ok()).unwrap_or(PAGE_SIZE);
            for item in data {
                out.push(item);
                if max_items.is_some_and(|m| out.len() >= m) {
                    return Ok(out);
                }
            }
            match page.get("nextCursor").and_then(Value::as_str) {
                Some(next) if page_len >= limit && Some(next) != cursor.as_deref() => cursor = Some(next.to_string()),
                _ => return Ok(out),
            }
        }
    }
}

/// Percent-encode a path parameter.
pub(crate) fn seg(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
