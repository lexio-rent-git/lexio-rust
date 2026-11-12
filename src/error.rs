//! Errors returned by the client.

use serde_json::Value;

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Any failure while calling the Lexio Rent API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The API answered with an error (`application/problem+json`).
    #[error("Lexio API error {status} ({}): {}", code.as_deref().unwrap_or("-"), detail.as_deref().unwrap_or("-"))]
    Api {
        /// HTTP status code.
        status: u16,
        /// Lexio error code, e.g. `VEHICLE_UNAVAILABLE` or `NOT_FOUND`.
        code: Option<String>,
        /// Human-readable detail.
        detail: Option<String>,
        /// Request id to quote to Lexio support.
        request_id: Option<String>,
        /// Seconds to wait, from `Retry-After` (429 and 503).
        retry_after: Option<u64>,
        /// Full problem document.
        problem: Value,
    },
    /// Network, TLS or decoding failure.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// No API key given and `LEXIO_API_KEY` is not set.
    #[error("missing API key: pass one or set LEXIO_API_KEY")]
    MissingApiKey,
}

impl Error {
    /// HTTP status for API errors.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Api { status, .. } => Some(*status),
            Error::Http(e) => e.status().map(|s| s.as_u16()),
            Error::MissingApiKey => None,
        }
    }

    /// Lexio error code for API errors.
    pub fn code(&self) -> Option<&str> {
        match self {
            Error::Api { code, .. } => code.as_deref(),
            _ => None,
        }
    }

    /// `true` for 404.
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }

    /// `true` for 409 (for example the vehicle is no longer available).
    pub fn is_conflict(&self) -> bool {
        self.status() == Some(409)
    }

    /// `true` for 429.
    pub fn is_rate_limited(&self) -> bool {
        self.status() == Some(429)
    }
}
