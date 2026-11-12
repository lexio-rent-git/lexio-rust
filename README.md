# lexio-rent

[![crates.io](https://img.shields.io/crates/v/lexio-rent.svg)](https://crates.io/crates/lexio-rent)
[![docs.rs](https://img.shields.io/docsrs/lexio-rent)](https://docs.rs/lexio-rent)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Async Rust client for the [Lexio Rent API](https://docs.lexio.rent) v1. It covers the **93 operations** of the API: fleet, catalogue, pricing, quotes, rentals, pick-up and return, payments, maintenance and the event feed.

<a href="https://lexio.rent/logiciel-location-voiture">Lexio Rent</a> is rental management software for professional car, van and two-wheeler rental companies: planning, contracts, check-in/check-out inspections, deposits and fleet maintenance. The API connects it to a booking site, an accounting system, a data warehouse or an AI agent.

## Install

```toml
[dependencies]
lexio-rent = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

## API key

In Lexio: **Settings → Developer → API tokens → New token**. Keys start with `lck_`. API access is part of the Business plan, and there is no sandbox: test against a dedicated organization.

```sh
export LEXIO_API_KEY="lck_..."
```

## Quick start

```rust,no_run
use lexio_rent::{Lexio, Params};
use serde_json::json;

#[tokio::main]
async fn main() -> lexio_rent::Result<()> {
    let lexio = Lexio::from_env()?; // or Lexio::new("lck_...")

    // Whole active fleet, page after page
    for v in lexio.vehicles().list_all(&Params::new().set("isArchived", false), None).await? {
        println!("{} {}", v["plate"], v["status"]);
    }

    // Price of every free vehicle for a weekend
    let window = Params::new()
        .set("startAt", "2027-03-05T09:00:00") // agency local time, no offset
        .set("endAt", "2027-03-07T18:00:00")
        .set("isAvailable", true);
    let offers = lexio.availability().list_all(&window, Some(10)).await?;

    // A quote, then a rental at the quoted price
    let quote = lexio.quotes().create(&json!({
        "vehicleId": offers[0]["vehicle"]["id"],
        "startAt": "2027-03-05T09:00:00",
        "endAt": "2027-03-07T18:00:00"
    }), None).await?;
    let rental = lexio.rentals().create(&json!({
        "quoteId": quote["data"]["id"],
        "customer": { "firstName": "Camille", "lastName": "Martin", "email": "camille@example.com" }
    }), None).await?;
    println!("rental {}", rental["data"]["number"]);
    Ok(())
}
```

A runnable version is in [`examples/fleet_and_quote.rs`](examples/fleet_and_quote.rs).

## What the client does for you

| Topic | Behaviour |
|---|---|
| Authentication | `Authorization: Bearer lck_…` on every request (the only method v1 accepts). |
| Idempotency | Writes that require an `Idempotency-Key` get a UUID v4 when you pass `None`. The same key is reused on every retry, so a rental is never created twice. Pass `Some("your-key")` to choose it. |
| Retries | On 429 and 5xx, up to 2 retries (`max_retries`), honouring `Retry-After`. Only for reads and idempotent writes. |
| Pagination | Every list method has a `*_all` twin that follows `nextCursor` (with an optional `max_items` cap). |
| Errors | `application/problem+json` responses become `Error::Api { status, code, detail, request_id, retry_after, problem }`, with `is_not_found()`, `is_conflict()`, `is_rate_limited()` helpers. |
| Data | Responses are `serde_json::Value`, with the API field names (`plate`, `startAt`, `totalTTC`...). |

```rust,no_run
# async fn f(lexio: lexio_rent::Lexio, body: serde_json::Value) {
match lexio.rentals().create(&body, None).await {
    Ok(rental) => println!("{}", rental["data"]["number"]),
    Err(e) if e.code() == Some("VEHICLE_UNAVAILABLE") => println!("pick another vehicle"),
    Err(e) => eprintln!("{e}"),
}
# }
```

## Resources

`organization`, `vehicles`, `vehicle_blocks`, `categories`, `options`, `deductibles`, `deposits`, `mileage_packages`, `locations`, `customers`, `availability`, `quotes`, `rentals` (27 operations: cancel, reschedule, change vehicle or customer, options, pick-up, return, photos, payments, invoices, contract...), `pricing_settings`, `seasons`, `season_calendar`, `duration_discounts`, `lead_time_tiers`, `maintenance_operations`, `maintenance_records`, `events`.

Each method is documented with its HTTP route and OpenAPI operation id. An operation not covered yet can be called with `lexio.request(Method::GET, "/path", Some(&params), None, None, false)`.

## Following changes (no webhooks)

The API has no webhooks. Read the `/events` feed with a cursor you keep between runs; see [Synchronisation](https://docs.lexio.rent/synchronisation).

## Development

```sh
python3 scripts/generate.py   # regenerate src/resources.rs from spec/openapi.json
cargo test                    # mocked HTTP server, no API key needed
```

## About

This crate is maintained by the team behind <a href="https://lexio.rent/logiciel-location-voiture">Lexio Rent, car rental management software</a>. Some rentals are not rentals at all: when a company lends a vehicle to an employee or a partner, Lexio publishes a free <a href="https://lexio.rent/outils/attestation-pret-vehicule">vehicle loan agreement generator</a> (in French) for that paperwork.

- API documentation: <https://docs.lexio.rent>
- OpenAPI specification: <https://docs.lexio.rent/openapi.json>
- Python client: <https://github.com/lexio-rent-git/lexio-python>
- Contact: contact@lexio.rent

MIT License.
