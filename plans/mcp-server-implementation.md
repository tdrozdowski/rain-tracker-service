# Rain Tracker MCP Server Implementation Plan

## Plan Metadata

**Plan ID**: `mcp-server-implementation`
**Version**: `1.0.0`
**Created**: 2025-01-15
**Last Updated**: 2025-01-15
**Status**: 📝 PLANNING
**Current Phase**: Phase 0 (Pre-implementation)

**Project Locations**:
- **Plan Location (Current)**: `<workspace-root>/rain-tracker-service/plans/mcp-server-implementation.md`
- **Plan Location (After Phase 1)**: `<workspace-root>/rain-tracker-mcp/PLAN.md`
- **Implementation Project**: `<workspace-root>/rain-tracker-mcp/`

**Migration Instructions**:
1. Complete Phase 1 (Project Setup) from this location
2. Copy this plan to the new project: `cp plans/mcp-server-implementation.md ../rain-tracker-mcp/PLAN.md`
3. Continue implementation from new project directory
4. Update status in new location as phases complete

---

## Overview

Create a separate Rust-based MCP (Model Context Protocol) server that provides AI assistants (like Claude) with tools to query rain gauge data from the Rain Tracker Service REST API. The project will use the official MCP Rust SDK (`rmcp`) and Rig for enhanced AI capabilities.

## Project Architecture Decision

**Architecture**: Separate project (recommended best practice)

**Rationale**:
- Clear separation of concerns between REST API and MCP protocol
- Independent deployment and scaling
- Simpler maintenance and testing
- The MCP server acts as a client of the REST API
- Different release cycles and versioning

## Project Structure

**Using Modern Rust Modules (2018+ Edition - No mod.rs)**:

```
rain-tracker-mcp/                    # New separate project
├── Cargo.toml
├── build.rs                         # Build script for OpenAPI generation
├── openapi.json                     # OpenAPI spec (copied from service)
├── README.md
├── .env.example
├── .gitignore
├── Dockerfile                       # Multi-stage Docker build
├── docker-compose.yml               # Full stack (postgres + api + mcp)
├── .github/
│   ├── workflows/
│   │   ├── ci.yml                   # Main CI pipeline
│   │   ├── docker.yml               # Docker build & push
│   │   ├── release.yml              # Release automation
│   │   ├── security-audit.yml       # Security scanning
│   │   └── codeql.yml               # CodeQL analysis (optional)
│   └── dependabot.yml               # Dependency updates
├── src/
│   ├── main.rs                      # MCP HTTP server entry point
│   ├── lib.rs                       # Library root (declares modules)
│   ├── generated.rs                 # Exposes generated client
│   ├── generated/                   # Generated code (gitignored)
│   │   └── client.rs                # Auto-generated from OpenAPI
│   ├── tools.rs                     # Tools module (declares submodules)
│   ├── tools/                       # Tool submodules
│   │   ├── gauges.rs                # Gauge-related tools
│   │   └── readings.rs              # Reading-related tools
│   ├── client.rs                    # Wrapper around generated client
│   ├── types.rs                     # Additional types (if needed)
│   └── config.rs                    # Configuration management
├── tests/
│   └── integration_test.rs          # Integration tests
├── examples/
│   └── test_tools.rs                # Example usage
├── http-tests/                      # HTTP endpoint tests (IntelliJ HTTP Client)
│   ├── http-client.env.json         # Environment variables
│   ├── health-check.http            # Health endpoint tests
│   ├── mcp-tool-gauges.http         # Gauge tool tests
│   ├── mcp-tool-readings.http       # Reading tool tests
│   └── error-cases.http             # Error handling tests
└── k8s/                             # Kubernetes manifests
    ├── namespace.yaml
    ├── configmap.yaml
    ├── deployment.yaml
    └── service.yaml
```

**Module Declaration in `src/lib.rs`**:
```rust
pub mod generated;  // Generated OpenAPI client
pub mod client;     // Wrapper around generated client
pub mod config;
pub mod types;
pub mod tools;
```

**Module Declaration in `src/generated.rs`**:
```rust
// Include the generated code
include!("generated/client.rs");
```

**Module Declaration in `src/tools.rs`**:
```rust
pub mod gauges;
pub mod readings;

// Re-export commonly used items
pub use gauges::*;
pub use readings::*;
```

## Benefits of OpenAPI + Progenitor Approach

✅ **Type Safety**: 100% type-safe client generated from spec
✅ **Zero Drift**: Client always matches API (regenerated on build)
✅ **Less Code**: No manual HTTP client implementation
✅ **Compile-Time Errors**: Catch API mismatches at compile time
✅ **Auto-Complete**: Full IDE support for all endpoints
✅ **Documentation**: Generated code includes doc comments from OpenAPI
✅ **Maintainability**: Update spec → rebuild → client stays in sync
✅ **Quality**: Progenitor used in production by Oxide (battle-tested)

## Dependencies

**Cargo.toml** (with latest versions as of January 2025):
```toml
[package]
name = "rain-tracker-mcp"
version = "0.1.0"
edition = "2021"

[dependencies]
# MCP Protocol - Latest: 0.8.1 (actively maintained, released Oct 2025)
rmcp = "0.8"

# Rig AI Framework - Latest: 0.22.0 (optional for Phase 5)
rig-core = { version = "0.22", optional = true }

# Async Runtime - Latest: 1.48.0
tokio = { version = "1.48", features = ["full"] }

# HTTP Client - Latest: 0.12.24
reqwest = { version = "0.12", features = ["json"] }
reqwest-middleware = "0.4"  # For middleware support

# HTTP Server (only needed for HTTP transport) - Latest: 0.8.1
axum = { version = "0.8", optional = true }
tower = { version = "0.4", optional = true }
tower-http = { version = "0.5", features = ["trace"], optional = true }

# OpenAPI Code Generation - Latest: 0.11.2
progenitor = "0.11"  # OpenAPI client generator

# Serialization - Latest: serde 1.0.228, serde_json 1.0.145
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Date/Time - Latest: 0.4.42
chrono = { version = "0.4", features = ["serde"] }

# Error Handling - Latest: anyhow 1.0.100, thiserror 2.0.17
anyhow = "1.0"
thiserror = "2.0"

# Configuration - Latest: 0.15.7
dotenvy = "0.15"

# Logging - Latest: tracing 0.1.41, tracing-subscriber 0.3.20
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Async trait support - Latest: 0.1.85
async-trait = "0.1"

[build-dependencies]
# Build-time OpenAPI client generation
progenitor = "0.11"

[dev-dependencies]
tokio-test = "0.4"

[features]
default = ["http"]  # HTTP transport for K8s deployment
http = ["dep:axum", "dep:tower", "dep:tower-http"]  # HTTP transport (K8s deployment)
rig = ["dep:rig-core"]  # Optional Rig integration for Phase 5
```

**Note**: HTTP transport is the default. The `axum`, `tower`, and `tower-http` dependencies are required for the HTTP server implementation.

**Version Notes**:
- `rmcp 0.8.1`: Official MCP Rust SDK, actively maintained (268 commits, 100+ contributors)
- `progenitor 0.11.2`: OpenAPI client generator from Oxide Computer Company (high quality, actively maintained)
- `axum 0.8.1`: Modern, ergonomic web framework for HTTP server (required)
- `tower 0.4.13`: Service abstraction for middleware (required for axum)
- `tower-http 0.5.2`: HTTP-specific middleware (tracing, CORS, etc.)
- `rig-core 0.22.0`: Optional - only needed for Phase 5 AI enhancements
- `reqwest-middleware 0.4.2`: For HTTP client middleware chains (retry, logging)
- `thiserror 2.0`: Major version bump from 1.x, ensure compatibility
- `chrono 0.4.42`: Date/time handling (matches rain-tracker-service)
- All other dependencies use latest stable versions

## OpenAPI Client Generation Strategy

**Using Progenitor** (Oxide Computer Company's OpenAPI generator):

### Why Progenitor?
1. **High Quality**: Used in production by Oxide Computer Company
2. **Type Safety**: Generates strongly-typed Rust clients
3. **Async First**: Built for tokio/async Rust
4. **Compile Time**: Generates code at build time from OpenAPI spec
5. **Well Maintained**: Active development, good documentation

### Build Process

Create `build.rs` in the MCP project root:
```rust
use progenitor::Generator;
use std::fs;

fn main() {
    // Read OpenAPI spec from rain-tracker-service
    let spec = fs::read_to_string("../rain-tracker-service/openapi.json")
        .expect("Failed to read OpenAPI spec");

    // Generate client code
    let mut generator = Generator::default();
    let code = generator
        .generate_text(&spec)
        .expect("Failed to generate client");

    // Write to src/generated/client.rs
    fs::create_dir_all("src/generated").unwrap();
    fs::write("src/generated/client.rs", code).unwrap();

    println!("cargo:rerun-if-changed=../rain-tracker-service/openapi.json");
}
```

This will:
1. Read the OpenAPI spec from the rain-tracker-service at build time
2. Generate a fully-typed Rust client
3. Regenerate if the OpenAPI spec changes

### Generated Client Usage

```rust
use crate::generated::client::Client;

let client = Client::new("http://localhost:8080/api/v1");

// Strongly typed, autocomplete-friendly calls
let gauges = client.gauges_list(Some(1), Some(50)).await?;
let gauge = client.gauges_get("59700").await?;
let readings = client.readings_water_year_get("59700", 2025).await?;
```

## MCP Tools to Implement

### 1. Gauge Management Tools

#### Tool: `list_gauges`
**Description**: Get a paginated list of all rain gauges

**Parameters**:
- `page` (optional, u32, default: 1): Page number
- `page_size` (optional, u32, default: 50): Items per page

**Returns**:
```json
{
  "total_gauges": 150,
  "page": 1,
  "page_size": 50,
  "total_pages": 3,
  "has_next_page": true,
  "has_prev_page": false,
  "gauges": [...]
}
```

**REST API Call**: `GET /api/v1/gauges?page={page}&page_size={page_size}`

---

#### Tool: `get_gauge_details`
**Description**: Get detailed information for a specific rain gauge

**Parameters**:
- `station_id` (required, string): The gauge station ID (e.g., "59700")

**Returns**:
```json
{
  "station_id": "59700",
  "gauge_name": "Cave Creek",
  "city_town": "Phoenix",
  "elevation_ft": 2400,
  "general_location": "Near Tatum Blvd",
  "rainfall_past_6h_inches": 0.15,
  "rainfall_past_24h_inches": 0.42,
  "last_scraped_at": "2025-01-15T10:30:00Z"
}
```

**REST API Call**: `GET /api/v1/gauges/{station_id}`

---

### 2. Reading Query Tools

#### Tool: `get_water_year_readings`
**Description**: Get rainfall readings for a gauge for a water year (Oct 1 - Sep 30)

**Parameters**:
- `station_id` (required, string): The gauge station ID
- `year` (required, i32): The water year (e.g., 2025 means Oct 1, 2024 - Sep 30, 2025)

**Returns**:
```json
{
  "water_year": 2025,
  "total_readings": 1234,
  "total_rainfall_inches": 12.45,
  "readings": [...]
}
```

**REST API Call**: `GET /api/v1/readings/{station_id}/water-year/{year}`

---

#### Tool: `get_calendar_year_readings`
**Description**: Get rainfall readings for a gauge for a calendar year with monthly breakdowns

**Parameters**:
- `station_id` (required, string): The gauge station ID
- `year` (required, i32): The calendar year (e.g., 2025)

**Returns**:
```json
{
  "calendar_year": 2025,
  "total_readings": 1234,
  "year_to_date_rainfall_inches": 8.75,
  "monthly_summaries": [...],
  "readings": [...]
}
```

**REST API Call**: `GET /api/v1/readings/{station_id}/calendar-year/{year}`

---

#### Tool: `get_latest_reading`
**Description**: Get the most recent rainfall reading for a specific gauge

**Parameters**:
- `station_id` (required, string): The gauge station ID

**Returns**:
```json
{
  "id": 12345,
  "reading_datetime": "2025-01-15T10:30:00Z",
  "cumulative_inches": 12.45,
  "incremental_inches": 0.04,
  "station_id": "59700",
  "created_at": "2025-01-15T10:31:00Z"
}
```

**REST API Call**: `GET /api/v1/readings/{station_id}/latest`

---

## Project Location & Directory Structure

### Where to Create the Project

The MCP server should be created as a **sibling** to `rain-tracker-service`:

```
<workspace-root>/
├── rain-tracker-service/          # Existing REST API service
│   ├── Cargo.toml
│   ├── src/
│   └── openapi.json               # Generated OpenAPI spec
│
└── rain-tracker-mcp/              # NEW MCP server (sibling project)
    ├── Cargo.toml
    ├── build.rs
    ├── src/
    └── openapi.json               # Copied from sibling
```

### Creating the Project

From the current directory (`rain-tracker-service`), navigate to parent workspace first:

```bash
cd ..                               # Navigate to parent workspace
cargo new rain-tracker-mcp
cd rain-tracker-mcp
```

**⚠️ Important**: Do NOT create the MCP project inside `rain-tracker-service`. They should be siblings.

### OpenAPI Spec Reference

Since the projects are siblings, the `build.rs` will reference the spec using relative path:

```rust
// In rain-tracker-mcp/build.rs
let spec = fs::read_to_string("../rain-tracker-service/openapi.json")
    .expect("Failed to read OpenAPI spec from sibling project");
```

Alternatively, copy the spec to the MCP project:

```bash
cp ../rain-tracker-service/openapi.json ./openapi.json
```

---

## Implementation Phases

### Phase 1: Project Setup & OpenAPI Client Generation

**Tasks**:
1. **Navigate to parent workspace**: `cd ..` (from rain-tracker-service directory)
2. **Create new Rust project**: `cargo new rain-tracker-mcp`
3. **Navigate into project**: `cd rain-tracker-mcp`
4. Set up project structure (create directories):
   - `mkdir -p src/tools src/generated tests examples http-tests k8s .github/workflows`
5. **Copy this plan to new project**:
   - `cp ../rain-tracker-service/plans/mcp-server-implementation.md ./PLAN.md`
   - Update status in PLAN.md to Phase 1
6. Configure `Cargo.toml` with dependencies including `progenitor`
7. Create `.env.example` with configuration template
8. Set up `.gitignore`:
   - Rust standard (target/, **/*.rs.bk)
   - `.env` (keep .env.example)
   - `src/generated/` (regenerated on build)
   - IDE files (.idea/, .vscode/)
9. **Export OpenAPI spec** from rain-tracker-service:
   - Ensure rain-tracker-service has generated OpenAPI spec
   - Copy spec: `cp ../rain-tracker-service/openapi.json ./openapi.json`
   - Or reference sibling path in `build.rs`
9. **Create `build.rs`** for OpenAPI client generation using progenitor:
   ```rust
   use progenitor::Generator;
   use std::fs;

   fn main() {
       // Option 1: Read from sibling project
       let spec = fs::read_to_string("../rain-tracker-service/openapi.json")
           .expect("Failed to read OpenAPI spec");

       // Option 2: Read from local copy
       // let spec = fs::read_to_string("openapi.json")
       //     .expect("Failed to read OpenAPI spec");

       let mut generator = Generator::default();
       let code = generator
           .generate_text(&spec)
           .expect("Failed to generate client");

       fs::create_dir_all("src/generated").unwrap();
       fs::write("src/generated/client.rs", code).unwrap();

       println!("cargo:rerun-if-changed=../rain-tracker-service/openapi.json");
   }
   ```
10. **Test build**: Verify `cargo build` generates client code successfully
11. Initialize logging infrastructure
12. Create basic error types

**Deliverables**:
- Project created at `<workspace-root>/rain-tracker-mcp/`
- Project skeleton compiles
- Generated client code in `src/generated/client.rs`
- Basic configuration loading works
- Logging is functional
- OpenAPI spec is accessible and up-to-date
- **Plan copied to new project**: `<workspace-root>/rain-tracker-mcp/PLAN.md`

**Post-Phase 1 Migration**:
After completing Phase 1, migrate to the new project:
```bash
# From rain-tracker-service directory
cd ..
cd rain-tracker-mcp

# Verify plan was copied
cat PLAN.md

# Update plan status to Phase 2
# Continue implementation from this directory
```

---

### Phase 2: Client Wrapper & Configuration

**Caching Strategy Decision**: No caching in v1.0
- Private, local network usage - low latency
- No SLA requirements
- AI conversational queries are low-frequency
- Prioritize simplicity and fresh data
- REST API performance is sufficient
- **Future**: Consider adding HTTP-level caching in v0.2.0 if needed

**Tasks**:
1. Create `src/generated.rs` module to expose generated client

2. **Define client trait** in `client.rs` for testability:
   - Create `RainTrackerClientTrait` with all API methods
   - Enables `mockall` trait-based mocking in tests
   - Both real client and mock implement same trait

3. Create wrapper in `client.rs` around generated client:
   - Configuration (base URL, timeout)
   - Middleware setup (retry, logging, error handling)
   - Custom error types wrapping generated errors
   - **No caching** - always fetch fresh from REST API
   - Implement `RainTrackerClientTrait` for wrapper

4. Add retry logic using `reqwest-middleware`:
   - Exponential backoff for 5xx errors
   - Retry on network failures
   - Max 3 retries

5. Add request/response logging for debugging

6. Add client initialization from environment config

7. Create convenience methods if needed

**Design Pattern for Mockability**:
```rust
use async_trait::async_trait;
use mockall::automock;

// Trait for abstraction (enables mockall)
// #[automock] automatically generates MockRainTrackerClientTrait
#[automock]
#[async_trait]
pub trait RainTrackerClientTrait: Send + Sync {
    async fn list_gauges(&self, page: u32, page_size: u32) -> Result<GaugeListResponse>;
    async fn get_gauge(&self, station_id: &str) -> Result<Gauge>;
    async fn get_water_year_readings(&self, station_id: &str, year: i32) -> Result<ReadingsResponse>;
    async fn get_calendar_year_readings(&self, station_id: &str, year: i32) -> Result<ReadingsResponse>;
    async fn get_latest_reading(&self, station_id: &str) -> Result<Reading>;
}

// Real implementation
pub struct RainTrackerClient {
    inner: generated::Client,
    config: ClientConfig,
}

#[async_trait]
impl RainTrackerClientTrait for RainTrackerClient {
    async fn list_gauges(&self, page: u32, page_size: u32) -> Result<GaugeListResponse> {
        // Implementation using generated client
        self.inner.gauges_list(Some(page), Some(page_size)).await
            .map_err(|e| ClientError::from(e))
    }

    // ... other implementations
}
```

**Usage in Tests**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;

    #[tokio::test]
    async fn test_list_gauges_tool() {
        // MockRainTrackerClientTrait is auto-generated by #[automock]
        let mut mock_client = MockRainTrackerClientTrait::new();

        mock_client
            .expect_list_gauges()
            .with(eq(1), eq(10))
            .times(1)
            .returning(|_, _| Ok(GaugeListResponse {
                total_gauges: 150,
                page: 1,
                page_size: 10,
                gauges: vec![],
            }));

        // Test MCP tool using mock client
        let result = mock_client.list_gauges(1, 10).await;
        assert!(result.is_ok());
    }
}
```

**Deliverables**:
- `RainTrackerClientTrait` for abstraction
- `RainTrackerClient` wrapper with configuration
- Retry and error handling middleware
- Clean API matching MCP tool needs
- Unit tests using `MockRainTrackerClientTrait`
- No caching layer (explicit decision)

---

### Phase 3: MCP Tool Implementations

**Tasks**:
1. Create `tools.rs` module (declares submodules)
2. Implement `tools/gauges.rs`:
   - `ListGaugesTool` with `#[derive(Tool)]` macro
   - `GetGaugeDetailsTool` with validation
3. Implement `tools/readings.rs`:
   - `GetWaterYearReadingsTool`
   - `GetCalendarYearReadingsTool`
   - `GetLatestReadingTool`
4. Add parameter validation:
   - Station ID format validation
   - Year range validation (e.g., 1900-2100)
   - Page/page_size bounds checking
5. Add comprehensive error messages for users
6. Add tool descriptions and parameter documentation

**Deliverables**:
- 5 fully implemented MCP tools
- All tools registered with the MCP server
- Proper validation and error handling

---

### Phase 4: MCP Server Setup (HTTP Transport)

**Tasks**:
1. Implement `main.rs` with Axum HTTP server
2. Set up HTTP endpoints:
   - `POST /mcp` - Main MCP protocol endpoint
   - `GET /health` - Health check endpoint
   - `GET /` - Optional info/status endpoint
3. Create `ServerHandler` and register all 5 tools
4. Implement MCP protocol handling over HTTP:
   - Parse incoming JSON-RPC style requests
   - Route to appropriate tool handler
   - Return structured JSON responses
5. Implement server lifecycle:
   - Graceful startup (bind to port 8081)
   - Tool routing and execution
   - Graceful shutdown on SIGINT/SIGTERM
   - Proper error handling and logging
6. Add configuration loading from environment:
   - `HTTP_PORT` (default: 8081)
   - `RAIN_TRACKER_API_URL`
   - `RUST_LOG`
7. Add structured logging with tracing
8. Add health check implementation (always returns 200 OK if running)

**Deliverables**:
- Fully functional HTTP-based MCP server binary
- Server listens on port 8081
- Health endpoint returns 200 OK
- All 5 tools are discoverable and executable via HTTP POST
- Proper JSON error responses
- Graceful shutdown works correctly

---

### Phase 5: Rig Integration (Optional Enhancement)

**Tasks**:
1. Integrate Rig framework for LLM capabilities
2. Add intelligent query interpretation:
   - Natural language to tool mapping
   - Parameter extraction from user queries
3. Add query planning:
   - Multi-step query orchestration
   - Suggest related queries
4. Add response enhancement:
   - Summarize large datasets
   - Format results for readability

**Deliverables**:
- Enhanced MCP server with AI capabilities
- Natural language query support
- Intelligent tool suggestions

**Note**: This phase is optional and can be deferred

---

### Phase 6: Testing & Documentation

**Tasks**:

**Testing**:
1. **Write unit tests** (80% coverage target):
   - HTTP client wrapper tests with `mockall` (trait-based mocking)
   - Tool parameter validation tests (all 5 tools)
   - Error handling tests (REST API failures)
   - Configuration loading tests
   - Add to `src/` files with `#[cfg(test)]` modules

2. **Write integration tests**:
   - End-to-end tool execution with `wiremock`
   - Server lifecycle tests (startup/shutdown)
   - Concurrent request handling
   - Error propagation tests
   - Create `tests/integration_test.rs`

3. **Create HTTP endpoint tests** (IntelliJ HTTP Client):
   - Create `http-tests/` directory structure
   - `http-client.env.json` with dev/docker/k8s environments
   - `health-check.http` - Health endpoint tests
   - `mcp-tool-gauges.http` - Gauge tool tests (list, get)
   - `mcp-tool-readings.http` - Reading tool tests (water year, calendar, latest)
   - `error-cases.http` - All error scenarios (invalid IDs, 404s, validation failures)

4. **Add test dependencies**:
   - `mockall = "0.13"` for trait-based unit test mocking
   - `wiremock = "0.6"` for HTTP server mocking in integration tests

5. **Run test suite**:
   - `cargo test --all-features`
   - `cargo clippy -- -D warnings`
   - `cargo fmt --check`

**Documentation**:
6. Create comprehensive `README.md`:
   - Project overview and architecture
   - Installation instructions (cargo, docker, k8s)
   - Configuration guide (environment variables)
   - HTTP endpoint documentation
   - Usage examples (curl and .http files)
   - Tool reference (all 5 MCP tools)
   - Development guide (running locally, testing)
   - Deployment guide (docker-compose, k8s)
   - Troubleshooting guide

7. Create `.env.example` with all configuration options

8. Document HTTP testing:
   - How to use `.http` files
   - Environment selection (dev/docker/k8s)
   - Expected responses

9. Create example in `examples/test_tools.rs` (programmatic usage)

10. Add inline documentation:
    - Rustdoc comments on all public APIs
    - Tool descriptions for MCP discovery

**Deliverables**:
- ✅ Unit test coverage > 80%
- ✅ Integration tests for all 5 tools
- ✅ Complete `.http` test suite (gauge tests, reading tests, error cases)
- ✅ Comprehensive README.md
- ✅ Working programmatic example
- ✅ All tests passing
- ✅ Ready for production deployment

---

## Configuration

### Environment Variables

**`.env` / `.env.example`**:
```bash
# HTTP Server Configuration
HTTP_PORT=8081

# REST API Configuration
RAIN_TRACKER_API_URL=http://localhost:8080/api/v1
RAIN_TRACKER_API_TIMEOUT=30

# Logging Level
RUST_LOG=info

# Optional: Future authentication
# RAIN_TRACKER_API_KEY=your_api_key_here

# Optional: Rig/LLM Configuration (if Phase 5 implemented)
# OPENAI_API_KEY=sk-...
# ANTHROPIC_API_KEY=sk-ant-...
```

### Remote Client Configuration

Since the MCP server uses HTTP transport, clients connect via HTTP rather than spawning a process.

**Example HTTP Client Configuration**:
```json
{
  "mcpServers": {
    "rain-tracker": {
      "url": "http://localhost:8081/mcp",
      "transport": "http"
    }
  }
}
```

**For remote access** (from K8s or another machine):
```json
{
  "mcpServers": {
    "rain-tracker": {
      "url": "http://rain-tracker-mcp.rain-tracker.svc.cluster.local:8081/mcp",
      "transport": "http"
    }
  }
}
```

**Note**: HTTP-based MCP support varies by client. For Claude Desktop v1.0, HTTP transport may require additional configuration or a proxy adapter. Future versions of this project (v0.2.0+) may add stdio support for local Claude Desktop usage.

---

## Example Usage

### Testing the HTTP MCP Server

**Health Check**:
```bash
curl http://localhost:8081/health
# Should return: 200 OK
```

**MCP Tool Invocation** (via HTTP POST):
```bash
curl -X POST http://localhost:8081/mcp \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
      "name": "list_gauges",
      "arguments": {
        "page": 1,
        "page_size": 10
      }
    },
    "id": 1
  }'
```

### Using the MCP Server with AI Clients

Once configured in an MCP client, users can ask natural language questions:

**Example Queries**:
- "List all rain gauges in the system"
- "Show me the details for gauge 59700"
- "Get water year 2025 rainfall data for gauge 59700"
- "What was the latest reading for Cave Creek gauge?"
- "Show me the calendar year 2024 data with monthly breakdowns for gauge 59700"

### Programmatic Usage (Example Code)

**`examples/test_tools.rs`**:
```rust
use rain_tracker_mcp::{RainTrackerClient, tools::*};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = RainTrackerClient::new("http://localhost:8080/api/v1")?;

    // List all gauges
    let gauges = client.list_gauges(1, 10).await?;
    println!("Found {} gauges", gauges.total_gauges);

    // Get specific gauge details
    let gauge = client.get_gauge_details("59700").await?;
    println!("Gauge: {}", gauge.gauge_name);

    // Get latest reading
    let reading = client.get_latest_reading("59700").await?;
    println!("Latest: {} inches", reading.cumulative_inches);

    Ok(())
}
```

---

## Benefits of This Architecture

### 1. Separation of Concerns
- REST API remains focused on its core responsibility (data access)
- MCP server focuses on AI assistant integration
- Each can evolve independently

### 2. Technology Flexibility
- Can use different languages/frameworks for each component
- Choose the best tool for each job
- Easier to adopt new technologies

### 3. Independent Scaling
- Scale REST API based on general traffic
- Scale MCP server based on AI assistant usage
- Different resource requirements

### 4. Simplified Testing
- Test REST API without MCP concerns
- Test MCP tools with mock REST API
- Easier unit and integration testing

### 5. Maintainability
- Smaller, focused codebases
- Easier to debug and troubleshoot
- Clear boundaries between components

### 6. Reusability
- MCP server is just another REST API client
- REST API can serve many different clients
- MCP tools can be reused across projects

### 7. AI-First Design
- Optimized for AI assistant interactions
- Tool descriptions tailored for LLMs
- Natural language query support (with Rig)

### 8. Security & Isolation
- Can run MCP server in different security context
- Easier to add authentication/authorization
- Network isolation between components

---

## Error Handling Strategy

### Client Errors (HTTP)
- Network failures → Retry with exponential backoff
- 404 Not Found → User-friendly "gauge not found" message
- 500 Server Error → Log and return "service temporarily unavailable"
- Timeout → Return "request timed out, please try again"

### Validation Errors
- Invalid station_id → "Invalid gauge ID format"
- Out-of-range year → "Year must be between 1900 and 2100"
- Invalid page_size → "Page size must be between 1 and 100"

### MCP Protocol Errors
- Tool not found → Should never happen (internal error)
- Invalid parameters → Return validation error to user
- Server errors → Log and return generic error

---

## Future Enhancements

### Short Term (v0.2.0)
1. **Add stdio transport** for local Claude Desktop usage:
   - Feature flag for stdio support
   - CLI argument to choose transport at runtime
   - Dual-mode binary for flexibility
2. **Add HTTP-level caching** if performance becomes an issue:
   - Use `http-cache` + `http-cache-reqwest` crates
   - Respect HTTP cache headers from REST API
   - Transparent, standards-based caching
   - Alternative: In-memory cache with `moka` (TTL: 1-5 minutes)
3. Add batch operations (query multiple gauges at once)
4. Add rate limiting to protect REST API
5. Add metrics/telemetry for monitoring (Prometheus)

### Medium Term (v0.3.0)
1. Implement advanced filtering (date ranges, rainfall thresholds)
2. Add data export tools (CSV, JSON)
3. Add data visualization suggestions
4. Implement webhook support for real-time updates

### Long Term (v1.0.0)
1. Full Rig integration for natural language queries
2. Multi-gauge comparison tools
3. Trend analysis and forecasting
4. Alert/notification system integration
5. GraphQL support for more flexible queries

---

## Security Considerations

### Current State
- No authentication (REST API is assumed to be internal/trusted)
- HTTP communication (local network)

### Future Enhancements
- Add API key authentication
- Implement HTTPS/TLS
- Add rate limiting per client
- Add audit logging
- Implement role-based access control (RBAC)

---

## Monitoring & Observability

### Logging
- Use `tracing` for structured logging
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Log all tool invocations with parameters
- Log HTTP requests/responses (at DEBUG level)
- Log errors with full context

### Metrics (Future)
- Tool invocation counts
- Response times per tool
- Error rates
- HTTP client metrics
- Cache hit/miss rates

### Health Checks
- MCP server startup/shutdown events
- REST API connectivity checks
- Periodic health check pings

---

## Deployment Options

### Development
- Run locally with `cargo run`
- Connect to local REST API at `localhost:8080`
- Use in Claude Desktop via local binary path

### Production Deployment Strategy

Since you plan to deploy to **Kubernetes alongside the REST API**, we'll provide:
1. **Dockerfile** for containerized MCP server
2. **docker-compose.yml** for local multi-service testing
3. **Kubernetes manifests** for production deployment

---

## Containerization & Orchestration

### Phase 7: Docker & Kubernetes Setup

**This phase should be added after Phase 6 (Testing & Documentation)**

#### Dockerfile for MCP Server

Create `Dockerfile` in rain-tracker-mcp project:

```dockerfile
# Multi-stage build for smaller image
FROM rust:1.85-slim as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source and build.rs
COPY build.rs ./
COPY src ./src
COPY openapi.json ./openapi.json

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install CA certificates for HTTPS
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/rain-tracker-mcp /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1001 mcpuser
USER mcpuser

# Set environment defaults
ENV RAIN_TRACKER_API_URL=http://rain-tracker-service:8080/api/v1
ENV RUST_LOG=info

ENTRYPOINT ["rain-tracker-mcp"]
```

---

#### docker-compose.yml for Local Development

Create `docker-compose.yml` in workspace root (`<workspace-root>/docker-compose.yml`):

```yaml
version: '3.8'

services:
  # PostgreSQL database
  postgres:
    image: postgres:18
    environment:
      POSTGRES_DB: rain_tracker
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: password
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

  # Rain Tracker REST API Service
  rain-tracker-service:
    build:
      context: ./rain-tracker-service
      dockerfile: Dockerfile
    environment:
      DATABASE_URL: postgres://postgres:password@postgres:5432/rain_tracker
      RUST_LOG: info
    ports:
      - "8080:8080"
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/api/v1/health"]
      interval: 10s
      timeout: 5s
      retries: 3

  # Rain Tracker MCP Server (HTTP)
  rain-tracker-mcp:
    build:
      context: ./rain-tracker-mcp
      dockerfile: Dockerfile
    environment:
      RAIN_TRACKER_API_URL: http://rain-tracker-service:8080/api/v1
      HTTP_PORT: 8081
      RUST_LOG: info
    ports:
      - "8081:8081"
    depends_on:
      rain-tracker-service:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 10s
      timeout: 5s
      retries: 3

volumes:
  postgres_data:
```

**Usage**:
```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop all services
docker-compose down
```

---

#### Kubernetes Manifests

Create `k8s/` directory in rain-tracker-mcp project:

**k8s/deployment.yaml**:
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rain-tracker-mcp
  namespace: rain-tracker
  labels:
    app: rain-tracker-mcp
spec:
  replicas: 2
  selector:
    matchLabels:
      app: rain-tracker-mcp
  template:
    metadata:
      labels:
        app: rain-tracker-mcp
    spec:
      containers:
      - name: mcp-server
        image: rain-tracker-mcp:latest
        imagePullPolicy: IfNotPresent
        ports:
        - containerPort: 8081
          name: http
          protocol: TCP
        envFrom:
        - configMapRef:
            name: rain-tracker-mcp-config
        resources:
          requests:
            memory: "64Mi"
            cpu: "100m"
          limits:
            memory: "128Mi"
            cpu: "200m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8081
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 8081
          initialDelaySeconds: 5
          periodSeconds: 10
```

**k8s/configmap.yaml**:
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: rain-tracker-mcp-config
  namespace: rain-tracker
data:
  RAIN_TRACKER_API_URL: "http://rain-tracker-service:8080/api/v1"
  HTTP_PORT: "8081"
  RUST_LOG: "info"
```

**k8s/service.yaml**:
```yaml
apiVersion: v1
kind: Service
metadata:
  name: rain-tracker-mcp
  namespace: rain-tracker
spec:
  selector:
    app: rain-tracker-mcp
  ports:
  - name: http
    port: 8081
    targetPort: 8081
    protocol: TCP
  type: ClusterIP
```

**k8s/namespace.yaml**:
```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: rain-tracker
```

**Deployment Commands**:
```bash
# Create namespace
kubectl apply -f k8s/namespace.yaml

# Apply configurations
kubectl apply -f k8s/configmap.yaml

# Deploy MCP server
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml

# Verify deployment
kubectl get pods -n rain-tracker
kubectl logs -f -n rain-tracker deployment/rain-tracker-mcp
```

---

#### Alternative: Kubernetes Sidecar Pattern

For tighter coupling, deploy MCP server as sidecar to REST API:

**k8s/rain-tracker-with-sidecar.yaml**:
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rain-tracker
  namespace: rain-tracker
spec:
  replicas: 2
  selector:
    matchLabels:
      app: rain-tracker
  template:
    metadata:
      labels:
        app: rain-tracker
    spec:
      containers:
      # Main REST API container
      - name: api
        image: rain-tracker-service:latest
        ports:
        - containerPort: 8080
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: rain-tracker-secret
              key: database_url

      # MCP server sidecar
      - name: mcp
        image: rain-tracker-mcp:latest
        envFrom:
        - configMapRef:
            name: rain-tracker-sidecar-config
        resources:
          requests:
            memory: "64Mi"
            cpu: "100m"
          limits:
            memory: "128Mi"
            cpu: "200m"
```

**Sidecar ConfigMap** (k8s/sidecar-configmap.yaml):
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: rain-tracker-sidecar-config
  namespace: rain-tracker
data:
  RAIN_TRACKER_API_URL: "http://localhost:8080/api/v1"  # Localhost within pod
  RUST_LOG: "info"
```

**Benefits of Sidecar**:
- Same pod = localhost communication (faster)
- Automatic scaling together
- Simplified networking
- Single deployment unit

---

### Phase 7 Tasks

**Tasks**:
1. Create `Dockerfile` for MCP server with multi-stage build
2. Test Docker build: `docker build -t rain-tracker-mcp:latest .`
3. Create `docker-compose.yml` in workspace root
4. Test docker-compose: full stack (postgres + api + mcp)
5. Create Kubernetes manifests in `k8s/` directory
6. Test K8s deployment in local cluster (minikube/kind)
7. Document deployment procedures in README
8. Add `.dockerignore` file
9. Consider image optimization (size, security)

**Deliverables**:
- Working Dockerfile
- docker-compose.yml for local development
- Complete Kubernetes manifests
- Deployment documentation
- Tested in local K8s cluster

---

## Testing Strategy

### Unit Tests (Rust)

**Location**: `src/` files with `#[cfg(test)]` modules

**What to test**:
1. **Parameter Validation**:
   - Test each tool's parameter validation logic
   - Test station_id format validation (e.g., alphanumeric)
   - Test year range validation (1900-2100)
   - Test page/page_size bounds checking
   - Test edge cases (empty strings, negative numbers, etc.)

2. **HTTP Client Wrapper** (with mocked responses):
   - Use `mockall` for trait-based mocking of client interface
   - Use `wiremock` for HTTP server mocking when needed
   - Test successful responses parsing
   - Test error handling (404, 500, timeouts)
   - Test retry logic with exponential backoff
   - Test connection failures

3. **Configuration Loading**:
   - Test environment variable parsing
   - Test default values
   - Test invalid configurations

4. **Error Handling**:
   - Test error type conversions
   - Test error messages for clarity
   - Test error propagation through layers

**Example Structure**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;

    #[tokio::test]
    async fn test_list_gauges_success() {
        // MockRainTrackerClientTrait auto-generated by #[automock]
        let mut mock_client = MockRainTrackerClientTrait::new();

        mock_client
            .expect_list_gauges()
            .with(eq(1), eq(10))
            .times(1)
            .returning(|_, _| Ok(GaugeListResponse {
                total_gauges: 150,
                page: 1,
                page_size: 10,
                has_next_page: true,
                has_prev_page: false,
                gauges: vec![],
            }));

        let result = mock_client.list_gauges(1, 10).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().total_gauges, 150);
    }

    #[tokio::test]
    async fn test_get_gauge_not_found() {
        let mut mock_client = MockRainTrackerClientTrait::new();

        mock_client
            .expect_get_gauge()
            .with(eq("99999"))
            .times(1)
            .returning(|_| Err(ClientError::NotFound("Gauge not found".to_string())));

        let result = mock_client.get_gauge("99999").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_station_id_validation() {
        assert!(validate_station_id("59700").is_ok());
        assert!(validate_station_id("").is_err());
        assert!(validate_station_id("invalid@id").is_err());
    }
}
```

---

### Integration Tests (Rust)

**Location**: `tests/` directory

**Purpose**: Test internal Rust code integration without HTTP layer
- Different from `.http` tests which test via actual HTTP endpoints
- Faster execution (no HTTP serialization overhead)
- Can test internal state and error paths not exposed via HTTP
- Runs in CI without needing Docker/containers

**What to test**:
1. **Tool Logic Integration** (without HTTP):
   - Test MCP tool implementations with mock client
   - Verify tool parameter validation
   - Test tool error handling and error message formatting
   - Test tool response mapping

2. **REST API Client Integration** (with wiremock):
   - Test client wrapper with mock REST API server
   - Test retry logic (simulate 500 errors, network failures)
   - Test timeout handling
   - Test response parsing edge cases

3. **Server Lifecycle** (programmatic):
   - Test server startup/shutdown logic
   - Test configuration loading
   - Test graceful shutdown (SIGTERM handling)
   - Test concurrent request handling (via direct function calls)

**Key Difference from .http Tests**:
- **Integration tests**: Test Rust code integration (tools + client + logic)
- **.http tests**: Test HTTP endpoints end-to-end (real HTTP server + serialization)

**Example Structure**:
```rust
// tests/integration_test.rs
use rain_tracker_mcp::*;
use wiremock::{MockServer, Mock, ResponseTemplate};

#[tokio::test]
async fn test_list_gauges_tool_with_mock_client() {
    // Test tool logic with mockall (no HTTP)
    let mut mock_client = MockRainTrackerClientTrait::new();

    mock_client
        .expect_list_gauges()
        .returning(|_, _| Ok(GaugeListResponse { /* ... */ }));

    // Test the MCP tool implementation directly
    let tool = ListGaugesTool::new(Arc::new(mock_client));
    let result = tool.execute(ListGaugesParams { page: 1, page_size: 10 }).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_client_retry_on_500() {
    // Test client retry logic with wiremock (HTTP server mock)
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/gauges"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&mock_server).await;

    Mock::given(method("GET"))
        .and(path("/gauges"))
        .respond_with(ResponseTemplate::new(200).set_body_json(...))
        .mount(&mock_server).await;

    let client = RainTrackerClient::new(mock_server.uri());
    let result = client.list_gauges(1, 10).await;

    // Should succeed after retries
    assert!(result.is_ok());
}
```

---

### HTTP Endpoint Tests (IntelliJ HTTP Client)

**Location**: `http-tests/` directory in project root

**Preference**: Use `.http` files instead of curl scripts for better maintainability

**Why IntelliJ HTTP Client over curl**:
- ✅ Version controlled `.http` files in the project
- ✅ Easy to maintain and update
- ✅ IDE integration (JetBrains IDEs, VS Code with REST Client extension)
- ✅ Environment variables support
- ✅ Response history and comparison
- ✅ Better developer experience than shell scripts

**Structure**:
```
http-tests/
├── http-client.env.json          # Environment variables
├── health-check.http             # Health endpoint tests
├── mcp-tool-gauges.http          # Gauge tool tests
├── mcp-tool-readings.http        # Reading tool tests
└── error-cases.http              # Error handling tests
```

**http-client.env.json**:
```json
{
  "dev": {
    "host": "localhost",
    "port": "8081",
    "api_url": "http://localhost:8081"
  },
  "docker": {
    "host": "localhost",
    "port": "8081",
    "api_url": "http://localhost:8081"
  },
  "k8s": {
    "host": "rain-tracker-mcp.rain-tracker.svc.cluster.local",
    "port": "8081",
    "api_url": "http://rain-tracker-mcp.rain-tracker.svc.cluster.local:8081"
  }
}
```

**health-check.http**:
```http
### Health Check
GET {{api_url}}/health

### Root Endpoint (optional info)
GET {{api_url}}/
```

**mcp-tool-gauges.http**:
```http
### List All Gauges (default pagination)
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "list_gauges",
    "arguments": {}
  },
  "id": 1
}

### List Gauges (page 2, 20 per page)
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "list_gauges",
    "arguments": {
      "page": 2,
      "page_size": 20
    }
  },
  "id": 2
}

### Get Gauge Details
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_gauge_details",
    "arguments": {
      "station_id": "59700"
    }
  },
  "id": 3
}
```

**mcp-tool-readings.http**:
```http
### Get Water Year Readings
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_water_year_readings",
    "arguments": {
      "station_id": "59700",
      "year": 2025
    }
  },
  "id": 4
}

### Get Calendar Year Readings
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_calendar_year_readings",
    "arguments": {
      "station_id": "59700",
      "year": 2024
    }
  },
  "id": 5
}

### Get Latest Reading
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_latest_reading",
    "arguments": {
      "station_id": "59700"
    }
  },
  "id": 6
}
```

**error-cases.http**:
```http
### Invalid Station ID
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_gauge_details",
    "arguments": {
      "station_id": ""
    }
  },
  "id": 7
}

### Invalid Year (out of range)
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_water_year_readings",
    "arguments": {
      "station_id": "59700",
      "year": 3000
    }
  },
  "id": 8
}

### Non-existent Tool
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "nonexistent_tool",
    "arguments": {}
  },
  "id": 9
}

### Invalid Page Size
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "list_gauges",
    "arguments": {
      "page": 1,
      "page_size": -5
    }
  },
  "id": 10
}

### Gauge Not Found (404)
POST {{api_url}}/mcp
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "get_gauge_details",
    "arguments": {
      "station_id": "99999"
    }
  },
  "id": 11
}
```

---

### Test Strategy Comparison

| Aspect | Unit Tests | Integration Tests (Rust) | HTTP Tests (.http files) |
|--------|-----------|-------------------------|--------------------------|
| **Location** | `src/` files with `#[cfg(test)]` | `tests/` directory | `http-tests/` directory |
| **What's tested** | Individual functions, validation | Rust code integration | HTTP endpoints end-to-end |
| **Dependencies** | mockall (trait mocks) | mockall + wiremock | Real HTTP server |
| **Execution speed** | Fastest (~ms) | Fast (~10-100ms) | Slower (~100-500ms) |
| **HTTP layer** | ❌ No | ❌ No | ✅ Yes |
| **Serialization** | ❌ No | ❌ No | ✅ Yes (JSON) |
| **Docker needed** | ❌ No | ❌ No | ✅ Optional (can use local) |
| **CI friendly** | ✅ Yes | ✅ Yes | ✅ Yes (with docker-compose) |
| **Manual testing** | ❌ No | ❌ No | ✅ Yes (click to run) |
| **Coverage tool** | cargo-llvm-cov | cargo-llvm-cov | Manual verification |
| **Best for** | Logic, validation, edge cases | Internal integration, retry logic | API contracts, real-world scenarios |

**Why Keep All Three?**:
1. **Unit tests**: Catch logic bugs early, fastest feedback
2. **Integration tests**: Verify Rust components work together, test retry/timeout logic
3. **HTTP tests**: Verify real HTTP behavior, manual testing, API contract validation

**Example Test Pyramid**:
```
        /\
       /  \    ← 5-10 HTTP tests (contracts, happy paths)
      /____\
     /      \  ← 20-30 Integration tests (component integration)
    /________\
   /          \ ← 50+ Unit tests (logic, validation, edge cases)
  /__________\
```

---

### Test Coverage Requirements

**Minimum Coverage Targets**:
- Unit tests: **80%** code coverage
- Integration tests: Cover all 5 MCP tools
- HTTP endpoint tests: All success and error paths
- Performance tests: Handle 100 concurrent requests

**What Must Be Tested**:
1. ✅ All 5 MCP tools (success cases)
2. ✅ All parameter validation rules
3. ✅ Error handling (REST API failures)
4. ✅ HTTP endpoints (health, MCP)
5. ✅ Configuration loading
6. ✅ Graceful shutdown
7. ✅ Concurrent request handling

**What Can Be Skipped**:
- Generated OpenAPI client code (already tested by Progenitor)
- Third-party library internals

---

### Manual Testing Checklist

**Local Development**:
- [ ] Start server: `cargo run`
- [ ] Health check works: `GET /health`
- [ ] Run all `.http` test files
- [ ] Verify responses match expected format
- [ ] Test with actual REST API running

**Docker**:
- [ ] Build image: `docker build -t rain-tracker-mcp .`
- [ ] Run container: `docker run -p 8081:8081 rain-tracker-mcp`
- [ ] Run health check against container
- [ ] Run `.http` tests against container

**docker-compose**:
- [ ] `docker-compose up`
- [ ] All services start successfully
- [ ] MCP server can reach REST API
- [ ] Run full test suite via `.http` files

**Kubernetes**:
- [ ] Deploy to test cluster
- [ ] Verify pods are running: `kubectl get pods -n rain-tracker`
- [ ] Check health: `kubectl exec -n rain-tracker <pod> -- curl localhost:8081/health`
- [ ] Port forward: `kubectl port-forward -n rain-tracker svc/rain-tracker-mcp 8081:8081`
- [ ] Run `.http` tests through port forward
- [ ] Verify Service DNS resolution
- [ ] Test from another pod in cluster

---

### Performance Testing

**Load Testing** (using `wrk` or `hey`):
```bash
# Install hey: go install github.com/rakyll/hey@latest

# Health endpoint load test
hey -n 10000 -c 100 -m GET http://localhost:8081/health

# MCP endpoint load test (with JSON payload)
hey -n 1000 -c 50 -m POST \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"tools/call","params":{"name":"list_gauges","arguments":{}},"id":1}' \
  http://localhost:8081/mcp
```

**Performance Targets**:
- Health endpoint: < 5ms p99 latency
- MCP tool calls: < 500ms p99 (depends on REST API)
- Throughput: > 100 req/sec on single instance
- Graceful degradation under load

---

### Testing Dependencies

Add to `Cargo.toml`:
```toml
[dev-dependencies]
tokio-test = "0.4"
mockall = "0.13"         # For trait-based unit test mocking
wiremock = "0.6"         # For HTTP server mocking in integration tests
```

**Why mockall over mockito**:
- ✅ Idiomatic Rust with proc macros
- ✅ Type-safe mocking with compile-time checks
- ✅ Better async support (works seamlessly with `#[async_trait]`)
- ✅ Trait-based mocking (more maintainable)
- ✅ Clear expectation syntax
- ✅ Active maintenance and community support
- ✅ **`#[automock]` macro**: Automatically generates mock implementations
  - Just add `#[automock]` above trait definition
  - Mock struct (`MockRainTrackerClientTrait`) is auto-generated
  - No manual `mock!{}` block needed
  - Compile-time errors if trait changes

---

### CI/CD Testing Strategy

**GitHub Actions / GitLab CI**:
1. **Unit Tests**: Run on every commit
2. **Integration Tests**: Run on every PR
3. **HTTP Tests**: Run against docker-compose stack in CI
4. **Coverage Report**: Generate with `cargo-llvm-cov`
5. **Linting**: `cargo clippy` must pass
6. **Formatting**: `cargo fmt --check` must pass

See the "GitHub Actions Workflows" section below for complete workflow definitions.

---

## GitHub Actions Workflows

### Recommended Workflows

Create `.github/workflows/` directory with the following workflow files:

**Workflow Structure**:
```
.github/
└── workflows/
    ├── ci.yml                  # Main CI pipeline (tests, lint, format)
    ├── docker.yml              # Build and push Docker images
    ├── release.yml             # Create releases and publish artifacts
    └── security-audit.yml      # Security vulnerability scanning
```

---

### 1. Main CI Pipeline (ci.yml)

**Triggers**: Push to main, all PRs
**Purpose**: Run tests, linting, formatting checks

```yaml
name: CI

on:
  push:
    branches: [ main, master ]
  pull_request:
    branches: [ main, master ]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  # Check formatting
  format:
    name: Format Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt

      - name: Check formatting
        run: cargo fmt --all -- --check

  # Linting with clippy
  clippy:
    name: Clippy Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy

      - name: Cache cargo registry
        uses: actions/cache@v4
        with:
          path: ~/.cargo/registry
          key: ${{ runner.os }}-cargo-registry-${{ hashFiles('**/Cargo.lock') }}

      - name: Cache cargo index
        uses: actions/cache@v4
        with:
          path: ~/.cargo/git
          key: ${{ runner.os }}-cargo-git-${{ hashFiles('**/Cargo.lock') }}

      - name: Cache cargo build
        uses: actions/cache@v4
        with:
          path: target
          key: ${{ runner.os }}-cargo-build-target-${{ hashFiles('**/Cargo.lock') }}

      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

  # Unit and integration tests
  test:
    name: Test Suite
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache dependencies
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Copy OpenAPI spec (if needed)
        run: |
          # If using sibling project reference, create dummy spec for CI
          echo '{"openapi":"3.0.0","info":{"title":"test","version":"1.0"}}' > openapi.json

      - name: Run unit tests
        run: cargo test --lib --all-features

      - name: Run integration tests
        run: cargo test --test '*' --all-features

  # Code coverage
  coverage:
    name: Code Coverage
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov

      - name: Copy OpenAPI spec
        run: echo '{"openapi":"3.0.0","info":{"title":"test","version":"1.0"}}' > openapi.json

      - name: Generate coverage
        run: cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

      - name: Upload to codecov.io
        uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: false

  # Integration test with docker-compose
  integration-docker:
    name: Integration Tests (Docker)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          # Need rain-tracker-service repo too for docker-compose
          path: rain-tracker-mcp

      - name: Build Docker image
        working-directory: rain-tracker-mcp
        run: docker build -t rain-tracker-mcp:test .

      - name: Start mock REST API (simplified)
        run: |
          # For CI, could run a mock server or skip this test
          # Or use docker-compose if rain-tracker-service is available
          echo "Skipping full stack test in CI - run locally"

      # Alternative: Run HTTP endpoint tests with mock
      - name: Start MCP server container
        run: |
          docker run -d --name mcp-test \
            -p 8081:8081 \
            -e RAIN_TRACKER_API_URL=http://localhost:8080/api/v1 \
            -e HTTP_PORT=8081 \
            rain-tracker-mcp:test

      - name: Wait for server health
        run: |
          timeout 30 bash -c 'until curl -f http://localhost:8081/health; do sleep 2; done'

      - name: Run HTTP health check
        run: curl -f http://localhost:8081/health

      - name: Cleanup
        if: always()
        run: docker stop mcp-test && docker rm mcp-test

  # Build check
  build:
    name: Build Check
    runs-on: ubuntu-latest
    strategy:
      matrix:
        rust: [stable, beta]
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust ${{ matrix.rust }}
        uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}

      - name: Copy OpenAPI spec
        run: echo '{"openapi":"3.0.0","info":{"title":"test","version":"1.0"}}' > openapi.json

      - name: Build
        run: cargo build --all-features --verbose
```

---

### 2. Docker Build & Push (docker.yml)

**Triggers**: Push to main, version tags
**Purpose**: Build and push Docker images to registry

```yaml
name: Docker Build & Push

on:
  push:
    branches: [ main, master ]
    tags: [ 'v*' ]
  pull_request:
    branches: [ main, master ]

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  build-and-push:
    name: Build and Push Docker Image
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Log in to Container Registry
        if: github.event_name != 'pull_request'
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=ref,event=branch
            type=ref,event=pr
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=sha,prefix={{branch}}-

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          context: .
          push: ${{ github.event_name != 'pull_request' }}
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
          platforms: linux/amd64,linux/arm64

      - name: Image digest
        run: echo ${{ steps.docker_build.outputs.digest }}
```

---

### 3. Release Workflow (release.yml)

**Triggers**: Version tags (v*)
**Purpose**: Create GitHub releases with binaries

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

env:
  CARGO_TERM_COLOR: always

jobs:
  create-release:
    name: Create Release
    runs-on: ubuntu-latest
    outputs:
      upload_url: ${{ steps.create_release.outputs.upload_url }}
    steps:
      - name: Create Release
        id: create_release
        uses: actions/create-release@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tag_name: ${{ github.ref }}
          release_name: Release ${{ github.ref }}
          draft: false
          prerelease: false

  build-release:
    name: Build Release Binary
    needs: create-release
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact_name: rain-tracker-mcp
            asset_name: rain-tracker-mcp-linux-amd64

          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            artifact_name: rain-tracker-mcp
            asset_name: rain-tracker-mcp-linux-arm64

          - os: macos-latest
            target: x86_64-apple-darwin
            artifact_name: rain-tracker-mcp
            asset_name: rain-tracker-mcp-darwin-amd64

          - os: macos-latest
            target: aarch64-apple-darwin
            artifact_name: rain-tracker-mcp
            asset_name: rain-tracker-mcp-darwin-arm64

    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          target: ${{ matrix.target }}

      - name: Install cross-compilation tools
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu

      - name: Copy OpenAPI spec
        run: echo '{"openapi":"3.0.0","info":{"title":"test","version":"1.0"}}' > openapi.json

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }}

      - name: Strip binary
        if: matrix.os == 'ubuntu-latest'
        run: strip target/${{ matrix.target }}/release/${{ matrix.artifact_name }}

      - name: Upload Release Asset
        uses: actions/upload-release-asset@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          upload_url: ${{ needs.create-release.outputs.upload_url }}
          asset_path: ./target/${{ matrix.target }}/release/${{ matrix.artifact_name }}
          asset_name: ${{ matrix.asset_name }}
          asset_content_type: application/octet-stream

  publish-crate:
    name: Publish to crates.io
    needs: create-release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Copy OpenAPI spec
        run: echo '{"openapi":"3.0.0","info":{"title":"test","version":"1.0"}}' > openapi.json

      - name: Publish to crates.io
        run: cargo publish --token ${{ secrets.CARGO_REGISTRY_TOKEN }}
        continue-on-error: true  # May fail if already published
```

---

### 4. Security Audit (security-audit.yml)

**Triggers**: Schedule (weekly), manual
**Purpose**: Scan for security vulnerabilities

```yaml
name: Security Audit

on:
  schedule:
    # Run every Monday at 9am UTC
    - cron: '0 9 * * 1'
  workflow_dispatch:
  push:
    paths:
      - '**/Cargo.toml'
      - '**/Cargo.lock'

jobs:
  security-audit:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run cargo-audit
        uses: actions-rust-lang/audit@v1
        with:
          # Ignore advisories for dev dependencies
          ignore: RUSTSEC-0000-0000

  dependency-review:
    name: Dependency Review
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4

      - name: Dependency Review
        uses: actions/dependency-review-action@v4
        with:
          fail-on-severity: moderate

  cargo-deny:
    name: Cargo Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Check licenses and advisories
        uses: EmbarkStudios/cargo-deny-action@v1
        with:
          log-level: warn
          command: check
          arguments: --all-features
```

---

### Additional Workflow Considerations

**5. Dependabot Configuration**

Create `.github/dependabot.yml`:
```yaml
version: 2
updates:
  # Rust dependencies
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 10
    groups:
      production-dependencies:
        patterns:
          - "*"
        exclude-patterns:
          - "dev-*"

  # GitHub Actions
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    open-pull-requests-limit: 5

  # Docker
  - package-ecosystem: "docker"
    directory: "/"
    schedule:
      interval: "weekly"
```

**6. CodeQL Analysis** (Optional)

Create `.github/workflows/codeql.yml`:
```yaml
name: CodeQL

on:
  push:
    branches: [ main, master ]
  pull_request:
    branches: [ main, master ]
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday

jobs:
  analyze:
    name: Analyze
    runs-on: ubuntu-latest
    permissions:
      security-events: write

    steps:
      - uses: actions/checkout@v4

      - name: Initialize CodeQL
        uses: github/codeql-action/init@v3
        with:
          languages: 'cpp'  # Rust compiled to native code

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Copy OpenAPI spec
        run: echo '{"openapi":"3.0.0","info":{"title":"test","version":"1.0"}}' > openapi.json

      - name: Build
        run: cargo build --release

      - name: Perform CodeQL Analysis
        uses: github/codeql-action/analyze@v3
```

---

### Workflow Summary

| Workflow | Trigger | Purpose | Required Secrets |
|----------|---------|---------|------------------|
| **ci.yml** | Push, PR | Tests, linting, coverage | None (optional: CODECOV_TOKEN) |
| **docker.yml** | Push to main, tags | Build & push Docker images | GITHUB_TOKEN (auto) |
| **release.yml** | Version tags (v*) | Create releases with binaries | CARGO_REGISTRY_TOKEN (optional) |
| **security-audit.yml** | Weekly, manual | Security scanning | None |
| **dependabot.yml** | Config file | Auto dependency updates | None |
| **codeql.yml** | Weekly, push | Security analysis | None |

---

### Required GitHub Secrets

Configure these in repository settings:

1. **CODECOV_TOKEN** (optional): For code coverage reports
   - Get from codecov.io after linking repository

2. **CARGO_REGISTRY_TOKEN** (optional): For publishing to crates.io
   - Get from crates.io account settings
   - Only needed if publishing crate

**Note**: `GITHUB_TOKEN` is automatically provided by GitHub Actions

---

### Branch Protection Rules

Recommended branch protection for `main`:

- ✅ Require pull request reviews (1 approver)
- ✅ Require status checks to pass:
  - `format`
  - `clippy`
  - `test`
  - `build`
- ✅ Require branches to be up to date
- ✅ Include administrators
- ✅ Restrict force pushes

---

## Success Criteria

### Minimum Viable Product (MVP)
- [x] All 5 tools implemented and working
- [x] MCP server starts and accepts connections
- [x] REST API client works correctly
- [x] Basic error handling
- [x] README with setup instructions

### Production Ready (v1.0)
- [x] Comprehensive test coverage (>80%)
- [x] Complete documentation
- [x] Proper error messages for all failure cases
- [x] Logging and observability
- [x] Performance testing completed
- [x] Security review completed

---

## Project Timeline

### Week 1: Foundation
- Phase 1: Project Setup (1 day)
- Phase 2: REST API Client (3 days)
- Initial testing (1 day)

### Week 2: Core Implementation
- Phase 3: MCP Tools (3 days)
- Phase 4: MCP Server (2 days)
- Integration testing (2 days)

### Week 3: Polish & Documentation
- Phase 6: Testing (2 days)
- Documentation (2 days)
- End-to-end testing with Claude Desktop (1 day)

### Optional: Week 4
- Phase 5: Rig Integration (3-5 days)
- Advanced testing and tuning

**Total Estimated Time**: 2-4 weeks (depending on scope)

---

## Repository Information

### Location
**Separate Repository**: `rain-tracker-mcp`

### Related Repositories
- `rain-tracker-service` - The REST API service
- Future: `rain-tracker-web` - Web UI (if created)

### Version Control
- Use semantic versioning (semver)
- Tag releases: `v0.1.0`, `v0.2.0`, etc.
- Keep CHANGELOG.md updated

---

## Plan Portability & Continuation

### Working Across Projects

This plan is designed to be **portable** between projects:

**Starting Location**: `rain-tracker-service/plans/mcp-server-implementation.md`
- Use this location for planning and Phase 1 execution
- Contains all implementation details
- Can reference sibling project paths

**After Phase 1**: `rain-tracker-mcp/PLAN.md`
- Copy plan to new project after setup
- Continue implementation from MCP project directory
- Update status table as you complete phases
- Keep plan in sync with implementation

### Migration Checklist

When moving to the new project after Phase 1:

```bash
# 1. Complete Phase 1 tasks in rain-tracker-service directory
cd <workspace-root>/rain-tracker-service

# 2. Verify new project was created successfully
ls -la ../rain-tracker-mcp/

# 3. Copy plan to new project (should happen in Phase 1, but verify)
cp plans/mcp-server-implementation.md ../rain-tracker-mcp/PLAN.md

# 4. Navigate to new project
cd ../rain-tracker-mcp

# 5. Update PLAN.md status
# - Mark Phase 1 as complete
# - Set Phase 2 as "In Progress"
# - Update dates in Phase Status Details table

# 6. Continue implementation
# From this point forward, work in rain-tracker-mcp directory
# and update PLAN.md as you complete tasks
```

### Keeping Plan Updated

As you work through phases, update the **Phase Status Details** table:

```markdown
| Phase | Status | Started | Completed | Notes |
|-------|--------|---------|-----------|-------|
| Phase 1: Project Setup | ✅ Complete | 2025-01-15 | 2025-01-15 | - |
| Phase 2: Client Wrapper | 🔄 In Progress | 2025-01-15 | - | Implementing trait |
```

### Git Strategy

**Option 1: Single Repo** (Monorepo)
```bash
# Keep both projects in same repo
git init
git add rain-tracker-service/ rain-tracker-mcp/
git commit -m "Initial setup: REST API and MCP server"
```

**Option 2: Separate Repos** (Recommended)
```bash
# rain-tracker-service repo
cd rain-tracker-service
git add plans/mcp-server-implementation.md
git commit -m "Add MCP server implementation plan"

# rain-tracker-mcp repo (new)
cd ../rain-tracker-mcp
git init
git add PLAN.md
git commit -m "Initial commit with implementation plan"
```

**Option 3: Keep Plan in Both**
```bash
# Update plan in original location
cd rain-tracker-service
git add plans/mcp-server-implementation.md
git commit -m "Update MCP implementation plan"

# Sync to MCP project
cp plans/mcp-server-implementation.md ../rain-tracker-mcp/PLAN.md

# Commit in MCP project
cd ../rain-tracker-mcp
git add PLAN.md
git commit -m "Sync plan updates from rain-tracker-service"
```

---

## Appendix: MCP Protocol Overview

### What is MCP?
Model Context Protocol (MCP) is a standard protocol that enables AI assistants to interact with external tools and data sources. It defines a client-server architecture where:
- **MCP Server**: Exposes tools and resources
- **MCP Client**: AI assistant (like Claude) that invokes tools

### Key Concepts

#### Tools
Functions that AI can call with structured parameters. Each tool has:
- Name (unique identifier)
- Description (tells AI what it does)
- Parameters (typed inputs with descriptions)
- Return type (structured output)

#### Transport
Communication layer between client and server:
- **stdio**: Standard input/output (most common for local tools)
- **HTTP**: For remote services
- **WebSocket**: For real-time bidirectional communication

#### Protocol Messages
- **Request**: Client asks server to execute a tool
- **Response**: Server returns result or error
- **Notification**: One-way messages for events

### Why MCP?
- **Standardization**: One protocol for all AI integrations
- **Discoverability**: Tools self-describe their capabilities
- **Type Safety**: Structured parameters and returns
- **Flexibility**: Works with any programming language

---

## Implementation Status Tracking

### Phase Completion Checklist

Update this section as you complete phases:

- [ ] **Phase 0: Planning** - Plan document created and reviewed
- [ ] **Phase 1: Project Setup** - Project created, dependencies configured
- [ ] **Phase 2: Client Wrapper** - REST API client wrapper implemented
- [ ] **Phase 3: MCP Tools** - All 5 MCP tools implemented
- [ ] **Phase 4: HTTP Server** - HTTP MCP server running
- [ ] **Phase 5: Rig Integration** - (Optional) AI enhancements
- [ ] **Phase 6: Testing & Docs** - Tests passing, documentation complete
- [ ] **Phase 7: Docker & K8s** - Containerization and orchestration ready

### Current Status

**Overall Status**: 📝 PLANNING

**Current Phase**: Phase 0 (Pre-implementation)

**Last Milestone**: Plan document created

**Next Milestone**: Begin Phase 1 (Project Setup)

### Phase Status Details

| Phase | Status | Started | Completed | Notes |
|-------|--------|---------|-----------|-------|
| Phase 0: Planning | 📝 In Progress | 2025-01-15 | - | This document |
| Phase 1: Project Setup | ⏳ Not Started | - | - | - |
| Phase 2: Client Wrapper | ⏳ Not Started | - | - | - |
| Phase 3: MCP Tools | ⏳ Not Started | - | - | - |
| Phase 4: HTTP Server | ⏳ Not Started | - | - | - |
| Phase 5: Rig Integration | ⏳ Not Started | - | - | Optional |
| Phase 6: Testing & Docs | ⏳ Not Started | - | - | - |
| Phase 7: Docker & K8s | ⏳ Not Started | - | - | - |

**Legend**: 📝 Planning | ⏳ Not Started | 🔄 In Progress | ✅ Complete | ⏸️ Paused | ❌ Blocked

### Next Steps

1. ✅ Review and approve this plan
2. ⏳ Execute Phase 1: Project Setup
3. ⏳ Copy plan to new project as `PLAN.md`
4. ⏳ Update status as implementation progresses

---

## Questions & Decisions

### Open Questions
1. Should we implement Phase 5 (Rig integration) in v1.0 or defer to v2.0?
2. Should we add a tool for "search gauges by name/location"?

### Transport Protocol Decision

**Status**: ✅ **DECIDED** - HTTP Transport

This is a critical architectural decision that affects:
- Deployment strategy (K8s manifests, docker-compose)
- Client integration (Claude Desktop vs remote access)
- Server implementation (Phase 4)

#### Option 1: stdio Transport

**How it works**:
- MCP server reads from stdin, writes to stdout
- Process-based communication
- Standard for Claude Desktop local integrations

**Pros**:
- ✅ Simple implementation
- ✅ Standard for MCP (most examples use this)
- ✅ Perfect for Claude Desktop local usage
- ✅ No network overhead
- ✅ Secure (process isolation)
- ✅ Easier debugging (can test with echo/pipes)

**Cons**:
- ❌ **Not suitable for K8s deployment** - stdio doesn't work well in containerized environments
- ❌ Can't access remotely (requires local process)
- ❌ No load balancing (one process = one client)
- ❌ Harder to scale horizontally
- ❌ Limited observability (no HTTP logs/metrics)

**Best for**:
- Local development with Claude Desktop
- Single-user scenarios
- Desktop applications

**K8s Implications**:
- Would require SSH/exec into pods to use
- Not practical for remote AI assistants
- Defeats purpose of K8s deployment

---

#### Option 2: HTTP Transport

**How it works**:
- MCP server listens on HTTP endpoint
- Clients connect via HTTP POST requests
- RESTful or JSON-RPC style communication

**Pros**:
- ✅ **Perfect for K8s deployment** - native HTTP support
- ✅ Remote access (any client on network)
- ✅ Load balancing via K8s Service
- ✅ Horizontal scaling (multiple replicas)
- ✅ Rich observability (HTTP logs, metrics, traces)
- ✅ Ingress support (external access if needed)
- ✅ Health checks work naturally

**Cons**:
- ❌ More complex implementation
- ❌ Requires HTTP server setup (Axum/warp)
- ❌ May need authentication layer
- ❌ Network latency (though minimal in same cluster)
- ❌ Claude Desktop requires custom configuration (MCP HTTP support varies)

**Best for**:
- Production K8s deployments
- Multi-user scenarios
- Remote AI assistants
- Microservices architectures

**K8s Implications**:
- Natural fit for Service/Ingress
- Easy health checks and readiness probes
- Standard deployment patterns apply

---

#### Option 3: Hybrid Approach (Both)

**Implementation**:
- Support both transports via feature flags or runtime config
- Use same tool implementations, different transport layers

**Pros**:
- ✅ Best of both worlds
- ✅ Flexibility for different deployment scenarios
- ✅ Can use stdio locally, HTTP in K8s

**Cons**:
- ❌ More code to maintain
- ❌ More complex testing
- ❌ Larger binary

**Example**:
```toml
[features]
default = ["stdio"]
stdio = []
http = ["dep:axum", "dep:tower"]
```

---

#### Decision: HTTP Transport ✅

**Rationale**:
1. ✅ K8s deployment is primary goal - HTTP is natural fit
2. ✅ Private network reduces auth complexity for v1.0
3. ✅ Better scalability and observability
4. ✅ Aligns with existing REST API architecture
5. ✅ Can still use locally (connect to http://localhost:8081)
6. ✅ Supports remote AI assistants and multi-user scenarios
7. ✅ Native K8s Service/Ingress integration

**Future**: Can add stdio transport in v0.2.0 if needed for Claude Desktop local integration

---

### Implementation Impact

With HTTP transport selected:
- ✅ Phase 4: HTTP server using Axum framework
- ✅ docker-compose: Standard HTTP service with port `8081:8081`
- ✅ K8s: Standard Deployment + Service + optional Ingress
- ✅ Testing: HTTP client tests using reqwest
- ✅ Health endpoint: `GET /health`
- ✅ MCP endpoint: `POST /mcp` (or similar)
- ✅ Proper liveness/readiness probes

---

### Decisions Made
- ✅ Use separate project architecture (sibling to rain-tracker-service)
- ✅ Use official MCP Rust SDK (`rmcp` 0.8.1)
- ✅ Use Progenitor for OpenAPI client generation
- ✅ Use Rig for AI enhancements (optional Phase 5)
- ✅ **Transport protocol: HTTP** (optimal for K8s deployment)
- ✅ No authentication in v1.0 (assume trusted local network)
- ✅ Focus on 5 core tools first
- ✅ **No caching in v1.0** - private local network, prioritize simplicity
- ✅ Modern Rust modules (no mod.rs files)
