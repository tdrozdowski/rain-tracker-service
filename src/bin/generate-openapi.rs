use rain_tracker_service::api::generate_openapi_spec;
use std::fs;

fn main() {
    let spec = generate_openapi_spec();
    let json = serde_json::to_string_pretty(&spec).expect("Failed to serialize OpenAPI spec");

    fs::write("openapi.json", json).expect("Failed to write openapi.json");
    println!("✅ Generated openapi.json");
}
