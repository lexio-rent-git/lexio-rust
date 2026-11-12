//! Async client for the [Lexio Rent API](https://docs.lexio.rent) v1.
//!
//! [Lexio Rent](https://lexio.rent/logiciel-location-voiture) is rental management software for car, van and
//! two-wheeler rental companies. This crate covers the 93 operations of the public API: fleet, catalogue,
//! pricing, quotes, rentals, pick-up and return, payments, maintenance and the event feed.
//!
//! ```no_run
//! use lexio_rent::{Lexio, Params};
//! use serde_json::json;
//!
//! # async fn run() -> lexio_rent::Result<()> {
//! let lexio = Lexio::from_env()?; // reads LEXIO_API_KEY
//!
//! let org = lexio.organization().get().await?;
//! println!("{}", org["data"]["name"]);
//!
//! // The whole active fleet, page after page
//! for vehicle in lexio.vehicles().list_all(&Params::new().set("isArchived", false), None).await? {
//!     println!("{} {}", vehicle["plate"], vehicle["status"]);
//! }
//!
//! // A quote, then a rental at the quoted price (idempotency key generated and reused on retry)
//! let quote = lexio.quotes().create(&json!({
//!     "vehicleId": "9c2d7e4a-5b1f-4d3c-8a6e-0f1b2c3d4e5f",
//!     "startAt": "2026-12-14T09:00:00",
//!     "endAt": "2026-12-16T18:00:00"
//! }), None).await?;
//! let rental = lexio.rentals().create(&json!({
//!     "quoteId": quote["data"]["id"],
//!     "customer": { "firstName": "Camille", "lastName": "Martin", "email": "camille@example.com" }
//! }), None).await?;
//! # let _ = rental;
//! # Ok(()) }
//! ```
#![warn(missing_docs)]

mod client;
mod error;
mod params;
mod resources;

pub use client::{Lexio, LexioBuilder, DEFAULT_BASE_URL};
pub use error::{Error, Result};
pub use params::Params;
pub use resources::*;
pub use reqwest::Method;
pub use serde_json::Value;
