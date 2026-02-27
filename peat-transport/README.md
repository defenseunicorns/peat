# peat-transport

HTTP/REST API transport layer for PEAT Protocol external integration.

## Overview

`peat-transport` provides external API access to CAP node state, enabling C2 dashboards, legacy systems, and monitoring tools to query the CAP mesh network via standard HTTP/REST endpoints.

## Features

- **HTTP/REST API**: Query nodes, cells, and beacons via REST endpoints
- **Read-only**: External systems can query state but not mutate (safety)
- **Backend agnostic**: Works with Ditto or Automerge+Iroh sync backends
- **JSON responses**: Uses peat-schema protobuf → JSON encoding
- **Per-node architecture**: Each CAP node runs its own HTTP server
- **Extensible**: Trait-based design for future WebSocket/gRPC support

## Quick Start

```rust
use cap_transport::http::Server;
use cap_protocol::sync::ditto::DittoBackend;
use cap_protocol::sync::DataSyncBackend;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize sync backend (Ditto)
    let backend = Arc::new(DittoBackend::new());
    backend.initialize(config).await?;

    // Start HTTP server
    let server = Server::new(backend)
        .bind("0.0.0.0:8080")
        .await?;

    println!("REST API listening on http://0.0.0.0:8080");
    server.serve().await?;

    Ok(())
}
```

## REST API Endpoints

### Health Check

```http
GET /api/v1/health
```

Returns API server health status.

**Response**:
```json
{
  "status": "healthy",
  "backend": "Ditto"
}
```

### Nodes

#### List all nodes

```http
GET /api/v1/nodes
```

**Query Parameters**:
- `phase` - Filter by protocol phase (discovery, cell, hierarchy)
- `health` - Filter by health status (nominal, degraded, critical, failed)

**Response**:
```json
{
  "nodes": [
    {
      "config": {
        "id": "node-1",
        "platform_type": "UAV",
        "capabilities": [...],
        "comm_range_m": 1000.0,
        "max_speed_mps": 10.0
      },
      "state": {
        "position": {
          "latitude": 37.7749,
          "longitude": -122.4194,
          "altitude": 100.0
        },
        "fuel_minutes": 120,
        "health": "HEALTH_STATUS_NOMINAL",
        "phase": "PHASE_CELL"
      }
    }
  ]
}
```

#### Get specific node

```http
GET /api/v1/nodes/{id}
```

**Response**: Single node object (same structure as above)

### Cells

#### List all cells

```http
GET /api/v1/cells
```

**Query Parameters**:
- `leader_id` - Filter by cell leader node ID

**Response**:
```json
{
  "cells": [
    {
      "config": {
        "id": "alpha",
        "min_size": 2,
        "max_size": 8
      },
      "state": {
        "leader_id": "node-1",
        "members": ["node-1", "node-2", "node-3"],
        "aggregated_capabilities": [...],
        "timestamp": "2025-11-06T12:00:00Z"
      }
    }
  ]
}
```

#### Get specific cell

```http
GET /api/v1/cells/{id}
```

**Response**: Single cell object (same structure as above)

### Beacons

#### Query beacons

```http
GET /api/v1/beacons
```

**Query Parameters**:
- `geohash_prefix` - Filter by geohash prefix (e.g., `9q8yy`)
- `operational` - Filter by operational status (true/false)

**Response**:
```json
{
  "beacons": [
    {
      "node_id": "node-1",
      "position": {...},
      "operational": true,
      "capabilities": [...],
      "fuel_remaining_pct": 0.75,
      "link_quality": 0.9,
      "timestamp": "2025-11-06T12:00:00Z"
    }
  ]
}
```

## Error Responses

All errors return JSON with error details:

```json
{
  "error": "Resource not found: node-99",
  "status": 404
}
```

**HTTP Status Codes**:
- `200 OK` - Successful request
- `400 Bad Request` - Invalid query parameters
- `404 Not Found` - Resource doesn't exist
- `500 Internal Server Error` - Backend or server error

## Architecture

```
External System (C2 Dashboard, ROS2, etc.)
          ↓ HTTP/REST
  ┌──────────────────────┐
  │  peat-transport       │
  │  (HTTP Server)       │
  └──────────────────────┘
          ↓ queries
  ┌──────────────────────┐
  │  peat-protocol        │
  │  (DataSyncBackend)   │
  └──────────────────────┘
          ↓ stores in
  ┌──────────────────────┐
  │  Ditto / Automerge   │
  │  (Sync Backend)      │
  └──────────────────────┘
```

## Examples

See `examples/` directory for complete examples:

- `simple_server.rs` - Basic HTTP server setup
- `query_client.rs` - Example client querying the API

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test
```

### Running Example

```bash
cargo run --example simple_server
```

## Future Enhancements

- WebSocket streaming for real-time updates
- gRPC API for typed RPC
- ROS2 DDS bridge for robotics integration
- Authentication and authorization
- Rate limiting and quotas

## License

MIT

## References

- [ADR-012: Schema Definition and Protocol Extensibility](../docs/adr/012-schema-definition-protocol-extensibility.md)
- [peat-schema](../peat-schema/README.md) - Protocol Buffer message definitions
- [peat-protocol](../peat-protocol/README.md) - Core PEAT Protocol implementation
