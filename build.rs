use std::fs;
use std::path::Path;

fn main() {
    // Tell Cargo to rerun this build script if src/api.rs changes
    println!("cargo:rerun-if-changed=src/api.rs");
    println!("cargo:rerun-if-changed=src/db/models.rs");
    println!("cargo:rerun-if-changed=src/services/gauge_service.rs");

    // Note: The actual OpenAPI spec generation happens at runtime
    // We create a placeholder here that will be updated by running the service
    let openapi_path = Path::new("openapi.json");

    if !openapi_path.exists() {
        // Create a placeholder file
        let placeholder = r#"{
  "note": "Run 'cargo run --bin generate-openapi' to generate the OpenAPI spec"
}"#;
        fs::write(openapi_path, placeholder).expect("Failed to create openapi.json placeholder");
    }
}
