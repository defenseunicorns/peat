# Peat Persistence

Storage abstraction layer for the Peat Protocol (CAP).

## Overview

`peat-persistence` provides a backend-agnostic interface for persisting and
querying Peat protocol data, enabling external systems to access CAP state
without coupling to a specific storage implementation.

The crate defines the `DataStore` trait and the query / event types used by
the rest of the workspace. A concrete backend based on Automerge + Iroh lives
in the `peat-protocol` storage module; additional backends can be added by
implementing `DataStore`.

## Features

- **Backend Agnostic**: `DataStore` trait abstracts over any CRDT / storage engine
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
  │  • Automerge + Iroh  │
  │  • SQLite (planned)  │
  │  • PostgreSQL (planned) │
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

## REST API

When built with the `external-api` feature, the crate provides HTTP endpoints
for querying CAP data.

### Endpoints

#### Health Check

```http
GET /api/v1/health
```

**Response:**
```json
{
  "status": "healthy",
  "store": "Automerge+Iroh",
  "version": "0.1.0"
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
    }
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

Build queries with the fluent API:

```rust
use peat_persistence::Query;

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

### Automerge + Iroh (current)

The production storage backend used by `peat-protocol` is built on Automerge
CRDTs with Iroh QUIC transport. It implements `DataStore` and is what
workspace binaries (peat-sim, peat-transport, FFI bindings) run against today.

### Historical Note

An earlier revision of the workspace also supplied a `DittoStore` backend
implementing `DataStore` against the proprietary Ditto SDK. That backend has
been removed; only Automerge + Iroh ships today. See
[ADR-011](../docs/adr/011-ditto-vs-automerge-iroh.md) for the historical
backend comparison.

### Planned Backends

- **SQLite**: Local persistence for testing
- **PostgreSQL**: Centralized storage for C2 systems

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
│   └── external/        # HTTP API (optional feature)
│       ├── mod.rs
│       ├── server.rs    # HTTP server
│       └── routes.rs    # API routes
├── Cargo.toml
└── README.md
```

### Adding a New Backend

To implement a new storage backend:

1. Create a new crate or module implementing `DataStore`
2. Add tests demonstrating conformance with the trait contract
3. Wire the backend into the host crate (e.g. `peat-protocol`) behind a
   feature flag

Example:

```rust
use peat_persistence::{DataStore, DocumentId, Query, Result};
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
        todo!()
    }
    // ... other methods
}
```

## Integration with Peat Ecosystem

`peat-persistence` is part of the larger Peat Protocol ecosystem:

- **peat-schema**: Protobuf message definitions (stores these messages)
- **peat-protocol**: Core protocol logic (provides the Automerge + Iroh backend and consumes this trait)
- **peat-transport**: HTTP/gRPC transports (complementary external access)

## Performance Considerations

- **Automerge + Iroh backend**: Typically <10ms for local queries
- **HTTP API**: Adds ~5-10ms overhead for serialization and transport
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
- [ ] SQLite backend for testing
- [ ] Query performance optimization
- [ ] Metrics and observability

## License

See the main Peat repository for license information.

## Contributing

Contributions are welcome! See `CONTRIBUTING.md` in the repository root for
guidelines.

## References

- [ADR-012: Schema Definition and Protocol Extensibility](../docs/adr/012-schema-definition-protocol-extensibility.md)
- [Peat Protocol Documentation](../README.md)
- [Automerge Documentation](https://automerge.org/docs/)
- [Iroh Documentation](https://iroh.computer/docs/)
