# CAP Persistence

Storage abstraction layer for the Capability Aggregation Protocol (CAP).

## Overview

`peat-persistence` provides a backend-agnostic interface for persisting and querying Peat protocol data, enabling external systems to access CAP state without coupling to specific storage implementations.

## Features

- **Backend Agnostic**: Works with Ditto, Automerge, or any CRDT backend via the `DataStore` trait
- **Query Interface**: Filter, sort, and paginate CAP data with type-safe queries
- **Live Updates**: Subscribe to real-time changes via observers
- **External API**: HTTP/REST endpoints for non-CAP systems (optional `external-api` feature)
- **Type Safe**: Strongly typed queries and results using Rust's type system

## Architecture

```text
External System (C2 Dashboard, Analytics)
          ↓ HTTP/REST
  ┌──────────────────────┐
  │  peat-persistence     │
  │  (External API)      │
  └──────────────────────┘
          ↓ uses
  ┌──────────────────────┐
  │  DataStore Trait     │
  └──────────────────────┘
          ↓ implemented by
  ┌──────────────────────┐
  │  Storage Backends    │
  │  • Ditto             │
  │  • Automerge (planned)│
  │  • SQLite (planned)  │
  └──────────────────────┘
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
peat-persistence = { path = "../peat-persistence" }

# Or with external API feature
peat-persistence = { path = "../peat-persistence", features = ["external-api"] }
```

## Quick Start

### Using the DataStore Trait

```rust
use cap_persistence::{DataStore, Query};
use cap_persistence::backends::DittoStore;
use cap_protocol::sync::ditto::DittoBackend;
use cap_protocol::sync::{BackendConfig, DataSyncBackend};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct NodeState {
    node_id: String,
    phase: String,
    health: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize Ditto backend
    let backend = Arc::new(DittoBackend::new());
    let config = BackendConfig {
        app_id: "my-cap-app".to_string(),
        persistence_dir: "/tmp/cap-data".into(),
        // ... other config
    };
    backend.initialize(config).await?;

    // Create persistence store
    let store = DittoStore::new(backend);

    // Save data
    let node = NodeState {
        node_id: "node-1".to_string(),
        phase: "discovery".to_string(),
        health: "nominal".to_string(),
    };
    let id = store.save("node_states", &node).await?;
    println!("Saved node with ID: {}", id);

    // Query data
    let nodes: Vec<NodeState> = store.query("node_states", Query::all()).await?;
    println!("Found {} nodes", nodes.len());

    // Subscribe to changes
    let mut stream = store.observe("node_states", Query::all()).await?;
    tokio::spawn(async move {
        while let Some(event) = stream.recv().await {
            println!("Change detected: {:?}", event);
        }
    });

    Ok(())
}
```

### Using the HTTP Server

```rust
use cap_persistence::external::Server;
use cap_persistence::backends::DittoStore;
use cap_protocol::sync::ditto::DittoBackend;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize backend and store
    let backend = Arc::new(DittoBackend::new());
    // backend.initialize(config).await?;

    let store = Arc::new(DittoStore::new(backend));

    // Create and start HTTP server
    let server = Server::new(store)
        .bind("0.0.0.0:8080")
        .await?;

    println!("REST API listening on http://0.0.0.0:8080");
    server.serve().await?;

    Ok(())
}
```

## REST API

When built with the `external-api` feature, the crate provides HTTP endpoints for querying CAP data.

### Endpoints

#### Health Check

```http
GET /api/v1/health
```

**Response:**
```json
{
  "status": "healthy",
  "store": "Ditto",
  "version": "4.12.0"
}
```

#### Query Collection

```http
GET /api/v1/collections/{name}?limit=10&offset=0
```

**Query Parameters:**
- `limit` - Maximum number of results
- `offset` - Pagination offset
- `sort_by` - Field to sort by (planned)
- `order` - Sort order: `asc` or `desc` (planned)

**Response:**
```json
{
  "collection": "node_states",
  "count": 5,
  "documents": [
    {
      "node_id": "node-1",
      "phase": "discovery",
      "health": "nominal"
    },
    ...
  ]
}
```

#### Get Document by ID

```http
GET /api/v1/collections/{name}/{id}
```

**Response:**
```json
{
  "collection": "node_states",
  "id": "abc123",
  "document": {
    "node_id": "node-1",
    "phase": "cell",
    "health": "nominal"
  }
}
```

### Error Responses

All endpoints return errors in a consistent format:

```json
{
  "error": "Document not found: abc123",
  "status": 404
}
```

**Status Codes:**
- `200` - Success
- `400` - Bad Request (invalid query)
- `404` - Not Found
- `500` - Internal Server Error

## DataStore Trait

The core abstraction for storage backends.

```rust
#[async_trait]
pub trait DataStore: Send + Sync {
    /// Save or update a document
    async fn save<T: Serialize + Send + Sync>(
        &self,
        collection: &str,
        document: &T,
    ) -> Result<DocumentId>;

    /// Query documents with filtering
    async fn query<T: DeserializeOwned + Send>(
        &self,
        collection: &str,
        query: Query,
    ) -> Result<Vec<T>>;

    /// Find a single document by ID
    async fn find_by_id<T: DeserializeOwned + Send>(
        &self,
        collection: &str,
        id: &DocumentId,
    ) -> Result<T>;

    /// Delete a document
    async fn delete(&self, collection: &str, id: &DocumentId) -> Result<()>;

    /// Subscribe to live updates
    async fn observe(
        &self,
        collection: &str,
        query: Query,
    ) -> Result<mpsc::UnboundedReceiver<ChangeEvent>>;

    /// Get store information
    fn store_info(&self) -> StoreInfo;
}
```

## Query Builder

Build complex queries with the fluent API:

```rust
use cap_persistence::Query;

let query = Query::new()
    .limit(10)
    .offset(0);

// Note: Filter support is planned but not yet implemented
// Full example when available:
// let query = Query::new()
//     .filter(Filter::Eq("phase", "cell"))
//     .filter(Filter::Gt("fuel_remaining_pct", 0.5))
//     .sort("created_at", SortOrder::Descending)
//     .limit(10);
```

## Storage Backends

### Ditto Backend

The current production implementation wraps the existing `DataSyncBackend` from `peat-protocol`.

```rust
use cap_persistence::backends::DittoStore;
use cap_protocol::sync::ditto::DittoBackend;
use std::sync::Arc;

let backend = Arc::new(DittoBackend::new());
let store = DittoStore::new(backend);
```

### Future Backends

Planned implementations:
- **Automerge + Iroh**: Open-source CRDT sync engine
- **SQLite**: Local persistence for testing
- **PostgreSQL**: Centralized storage for C2 systems

## Examples

See the `examples/` directory for complete working examples:

```bash
# Basic usage
cargo run --example basic_usage

# HTTP server
cargo run --example http_server --features external-api
```

## Testing

Run the test suite:

```bash
# All tests
cargo test

# With external API features
cargo test --features external-api
```

## Development

### Project Structure

```
peat-persistence/
├── src/
│   ├── lib.rs           # Main library entry point
│   ├── error.rs         # Error types
│   ├── types.rs         # Core types (Query, DocumentId, etc.)
│   ├── store.rs         # DataStore trait
│   ├── backends/        # Storage backend implementations
│   │   ├── mod.rs
│   │   └── ditto.rs     # Ditto adapter
│   └── external/        # HTTP API (optional feature)
│       ├── mod.rs
│       ├── server.rs    # HTTP server
│       └── routes.rs    # API routes
├── examples/            # Usage examples
├── Cargo.toml
└── README.md
```

### Adding a New Backend

To implement a new storage backend:

1. Create a new file in `src/backends/`
2. Implement the `DataStore` trait
3. Add to `src/backends/mod.rs`
4. Add tests

Example:

```rust
// src/backends/sqlite.rs
use crate::store::DataStore;
use async_trait::async_trait;

pub struct SqliteStore {
    // ...
}

#[async_trait]
impl DataStore for SqliteStore {
    async fn save<T: Serialize + Send + Sync>(
        &self,
        collection: &str,
        document: &T,
    ) -> Result<DocumentId> {
        // Implementation
    }
    // ... other methods
}
```

## Integration with CAP Ecosystem

`peat-persistence` is part of the larger Peat Protocol ecosystem:

- **peat-schema**: Protobuf message definitions (stores these messages)
- **peat-protocol**: Core protocol logic (uses this for state storage)
- **peat-transport**: HTTP/gRPC transports (complementary external access)

## Performance Considerations

- **Ditto Backend**: Performance depends on Ditto SDK (typically <10ms for local queries)
- **HTTP API**: Add ~5-10ms overhead for serialization and transport
- **Observers**: Near real-time updates (typically <100ms latency)

## Security

When deploying the HTTP API:

- Use TLS for production deployments
- Implement authentication (planned feature)
- Apply rate limiting
- Restrict CORS origins in production

## Roadmap

- [ ] Advanced query filters (Eq, Gt, Lt, Contains, etc.)
- [ ] Authentication and authorization middleware
- [ ] WebSocket streaming API for live updates
- [ ] Automerge backend implementation
- [ ] SQLite backend for testing
- [ ] Query performance optimization
- [ ] Metrics and observability

## License

See the main CAP repository for license information.

## Contributing

Contributions are welcome! See `DEVELOPMENT.md` in the repository root for guidelines.

## References

- [ADR-012: Schema Definition and Protocol Extensibility](../docs/adr/012-schema-definition-protocol-extensibility.md)
- [Peat Protocol Documentation](../README.md)
- [Ditto SDK Documentation](https://docs.ditto.live/)
