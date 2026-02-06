# ADR-052: LLM Runtime Strategy for Agentic Simulation

**Status**: Proposed
**Date**: 2026-02-06
**Authors**: Kit Plummer (via Gastown)
**Extends**: ADR-051 Addendum A (Agentic Simulation Architecture)
**Relates to**: ADR-022 (Edge MLOps — MCP Bridge), hive-inference crate

## Context

ADR-051 Addendum A proposes an agentic simulation where each ContainerLab container runs a Gastown AI agent. Each agent needs access to an LLM for the Orient/Decide phases of the OODA loop. The addendum identifies three options (shared API, local small models, tiered hybrid) but doesn't make a definitive selection.

This ADR evaluates the current (early 2026) landscape and recommends a concrete runtime strategy.

## Decision

**Tiered hybrid architecture with portable LLM provider abstraction.**

### Phase 0: API-First

All agents use a cloud API (Anthropic Claude or OpenAI-compatible) via HTTP. This is the fastest path to proving the agent-MCP-HIVE loop works.

### Phase 1+: Hybrid Local + API

- **Simple entities** (sensors, tractors, lashing crew): Embedded inference via `llama-cpp-2` Rust binding (already in `hive-inference` crate) running Qwen3-0.6B or Qwen3-1.7B quantized models.
- **Complex entities** (crane operators, schedulers, aggregators): Shared inference server (Ollama or LocalAI) running Qwen3-8B.
- **Fallback**: Cloud API for any entity needing capabilities beyond local models.

## Evaluation

### Runtime Options Evaluated

| Runtime | Type | Docker Support | Tool Calling | MCP Support | Memory (8B Q4) | Best For |
|---------|------|---------------|-------------|-------------|----------------|----------|
| **Ollama** | Server | Excellent | Yes (native) | No | ~6 GB | Simplest shared server |
| **llama.cpp** | Library/Server | Good | Yes (jinja) | No | ~4.5 GB | Embedded via Rust FFI |
| **vLLM** | Server (GPU) | Excellent | Yes | No | ~8 GB VRAM | High-throughput GPU serving |
| **LocalAI** | Server | Excellent | Yes | **Yes (native)** | ~5 GB | MCP-native local inference |
| **Docker Model Runner** | Docker-native | Native | Backend-dependent | No | ~5 GB | Compose-integrated models |

### Key Finding: Existing llama-cpp-2 Integration

The `hive-inference` crate already declares `llama-cpp-2` as an optional dependency:

```toml
# hive-inference/Cargo.toml
llama-cpp-2 = { version = "0.1", optional = true }

[features]
llm-inference = ["llama-cpp-2"]
llm-cuda = ["llm-inference", "llama-cpp-2/cuda"]
```

This means we can embed LLM inference directly into the Rust simulation binary for simple entities — no Python, no HTTP overhead, no separate process. This is the most efficient path for simple agents that need minimal reasoning.

### Model Recommendations

| Entity Complexity | Model | Parameters | Quantized Size | RAM | Reasoning Quality |
|-------------------|-------|-----------|---------------|-----|-------------------|
| Simple (sensors, tractors) | Qwen3-0.6B Q4_K_M | 0.6B | ~400 MB | ~1 GB | Basic tool selection |
| Moderate (workers, cranes) | Qwen3-1.7B Q4_K_M | 1.7B | ~1 GB | ~2 GB | Good tool calling |
| Complex (schedulers, aggregators) | Qwen3-8B Q4_K_M | 8.2B | ~4.5 GB | ~6 GB | Strong reasoning + tools |

The Qwen3 family is recommended because:
- Native tool/function calling support across all sizes
- Thinking/non-thinking modes (skip CoT for simple decisions)
- Consistent architecture from 0.6B to 235B
- GGUF availability for llama.cpp compatibility

### Memory Budget

| Scale | Simple (embedded 0.6B) | Complex (shared 8B) | Total |
|-------|----------------------|---------------------|-------|
| 15 nodes (Phase 1) | 10 x 1 GB = 10 GB | 1 server x 6 GB | ~16 GB |
| 58 nodes (Phase 2) | 40 x 1 GB = 40 GB | 2 servers x 6 GB | ~52 GB |
| 200 nodes (Phase 3) | 160 x 1 GB = 160 GB | 4 servers x 6 GB | ~184 GB |

Phase 3 at 200 nodes with per-container embedded models pushes memory limits. At that scale, we should shift most simple entities to a shared small-model server (reducing to ~30 GB) or use rule-based logic for sensors/tractors (no LLM needed).

### Architecture: Per-Container vs. Shared Server

**Recommended: Hybrid**

```
[Shared Ollama/LocalAI: Qwen3-8B]
    ^        ^        ^
    |        |        |
[Crane 1] [Crane 2] [Scheduler]    <- Complex entities (HTTP calls)

[Tractor + embedded Qwen3-0.6B]    <- Simple entities (in-process)
[Sensor: rule-based, no LLM]       <- Trivial entities (no LLM)
```

Rationale:
- Sensors and simple actuators don't need LLMs — rule-based logic suffices
- Tractors and workers benefit from lightweight local models for adaptability
- Crane operators and schedulers need strong reasoning and tool calling
- A shared server avoids loading 8B models 15+ times

### LocalAI vs. Ollama for Shared Server

**Ollama**: Simpler setup, larger community, better model management.
**LocalAI**: Native MCP support, constrained grammars for structured output, P2P distributed inference.

For this project, **LocalAI's native MCP support** is compelling — it aligns with the MCP bridge architecture and could simplify the integration path. However, Ollama's simplicity makes it the safer Phase 0/1 choice.

**Decision**: Start with Ollama. Evaluate LocalAI for Phase 2+ when MCP integration matters more.

### Docker Model Runner

Docker Model Runner (Compose `provider.type: model`) is a promising integration path. When mature, it could replace the explicit Ollama service with a Compose-native model definition:

```yaml
# Future: models as first-class Compose services
services:
  inference:
    provider:
      type: model
      options:
        model: qwen3:8b
```

Monitor for Phase 2+.

## Consequences

### Positive
- Phase 0 runs with zero local infrastructure (just API key)
- `llama-cpp-2` integration reuses existing hive-inference dependency
- Hybrid approach scales from 15 to 200 nodes within workstation memory
- Provider abstraction allows runtime switching without code changes

### Negative
- API costs during Phase 0 development (~$0.01-0.05 per OODA cycle with Claude Haiku)
- Multiple model runtimes to manage in Phase 1+ (embedded + shared server)
- Qwen3 model quality may not match Claude for complex scheduling scenarios

### Risks
- Local model tool-calling quality: Qwen3-0.6B may struggle with nuanced decisions. Mitigation: constrained tool schemas, non-thinking mode, fallback to larger model.
- Memory pressure at scale: 200 containers with embedded models is aggressive. Mitigation: shift to shared serving or rule-based for simple entities.

## References

- hive-inference Cargo.toml (existing llama-cpp-2 dependency)
- [Ollama Docker](https://hub.docker.com/r/ollama/ollama)
- [LocalAI MCP Support](https://localai.io/features/mcp/)
- [Docker Model Runner](https://docs.docker.com/ai/model-runner/)
- [Qwen3 Model Family](https://qwenlm.github.io/blog/qwen3/)

---

**Decision Record:**
- **Proposed:** 2026-02-06
- **Accepted:** TBD

**Authors:** Kit Plummer (via Gastown Mayor agent)
