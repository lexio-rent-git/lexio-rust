//! Lists the active fleet, checks availability for a weekend and creates a quote.
//!
//! ```sh
//! export LEXIO_API_KEY="lck_..."
//! cargo run --example fleet_and_quote
//! ```
use lexio_rent::{Lexio, Params};
use serde_json::json;

#[tokio::main]
async fn main() -> lexio_rent::Result<()> {
    let lexio = Lexio::from_env()?;
    println!("Organization: {}", lexio.organization().get().await?["data"]["name"]);

    let fleet = lexio.vehicles().list_all(&Params::new().set("isArchived", false), None).await?;
    println!("{} active vehicles", fleet.len());

    let window = Params::new().set("startAt", "2027-03-05T09:00:00").set("endAt", "2027-03-07T18:00:00").set("isAvailable", true);
    let offers = lexio.availability().list_all(&window, Some(5)).await?;
    for offer in &offers {
        println!("{} {}: {} {}", offer["vehicle"]["name"], offer["vehicle"]["plate"], offer["totalTTC"], offer["currency"]);
    }

    if let Some(first) = offers.first() {
        let quote = lexio
            .quotes()
            .create(
                &json!({"vehicleId": first["vehicle"]["id"], "startAt": "2027-03-05T09:00:00", "endAt": "2027-03-07T18:00:00"}),
                None,
            )
            .await?;
        println!("Quote {} : {} {}", quote["data"]["id"], quote["data"]["totalTTC"], quote["data"]["currency"]);
    }
    Ok(())
}
