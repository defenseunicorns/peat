# ADR-053: Voice and Audio Messages in the PEAT CRDT Landscape

**Status**: Proposed
**Date**: 2026-02-26
**Authors**: Kit Plummer, Codex
**Organization**: (r)evolve - Revolve Team LLC (https://revolveteam.com)
**Relates To**: ADR-021 (Document-Oriented Architecture), ADR-025 (Blob Transfer Protocol), ADR-032 (Pluggable Transport Abstraction), ADR-035 (PEAT-Lite Embedded Nodes), ADR-037 (Resource-Constrained Device Optimization), ADR-039 (PEAT-BTLE Mesh Transport), ADR-044 (Encryption), ADR-051 (PEAT-SBD Satellite Transport), ADR-052 (PEAT-LoRa Transport)

---

## Executive Summary

This ADR defines the concept and requirements for voice and audio messages as a first-class data type in PEAT. Inspired by the [TerminalPhone](https://gitlab.com/here_forawhile/terminalphone) project — a Bash script providing anonymous encrypted push-to-talk (PTT) voice over Tor hidden services — this document captures how discrete audio clips fit naturally into PEAT's CRDT-based synchronization model. A voice message is an immutable binary blob with metadata, exactly like a PLI report or sensor reading: record it, encode it, publish it as a CRDT document referencing a BlobStore entry, and let the mesh sync it. This is not real-time streaming voice — it is the record-then-send PTT model, where each audio clip is a discrete, self-contained artifact that syncs across transports at whatever bandwidth is available.

---

## Context

### The Voice Gap

PEAT Protocol synchronizes structured data (PLI, status, sensor readings, AI products) across a multi-transport mesh. But field operators communicate primarily by voice — and today that voice traffic flows through entirely separate radio systems (VHF/UHF tactical radios, cell phones, satellite phones) that are disconnected from the PEAT data mesh. This creates several problems:

| Problem | Impact |
|---------|--------|
| Voice lives outside the data mesh | No searchability, no persistence, no CRDT sync |
| Radio channels are ephemeral | Missed transmissions are lost forever |
| No transcript integration | Voice intel requires manual transcription |
| Separate encryption domains | PEAT data encrypted one way, voice another |
| No transport flexibility | Voice locked to one radio, can't failover |

### Why PTT (Record-Then-Send), Not Streaming

Real-time voice streaming (VoIP, WebRTC, RTP) requires dedicated low-latency transport with jitter buffers, packet loss concealment, and sustained bandwidth — fundamentally incompatible with PEAT's store-and-forward, multi-transport, CRDT-based architecture. The PTT model maps naturally to PEAT because:

| Property | PTT (Record-Then-Send) | Streaming (Real-Time) |
|----------|------------------------|----------------------|
| **Data model** | Discrete blob (immutable) | Continuous stream (mutable) |
| **CRDT fit** | Perfect — blob ref in document | Poor — no merge semantics for streams |
| **Transport** | Any (QUIC, BLE, LoRa, SBD) | Requires low-latency (QUIC only) |
| **Latency tolerance** | Seconds to minutes | < 200ms required |
| **Bandwidth** | Adapts to transport | Fixed minimum required |
| **Offline** | Works — sync when connected | Fails without connection |
| **Persistence** | Automatic — it's a blob | Requires separate recording |
| **Searchability** | Transcribe → text search | Requires separate pipeline |

A 10-second PTT message at Opus 16kbps mono is ~20 KB — smaller than many CRDT documents. It can sync over BLE, LoRa, or even SBD satellite.

### TerminalPhone as Prior Art

[TerminalPhone](https://gitlab.com/here_forawhile/terminalphone) is a Bash script that provides anonymous encrypted PTT voice communication over Tor hidden services. Its design decisions are directly relevant to PEAT:

| TerminalPhone Concept | PEAT Equivalent |
|-----------------------|-----------------|
| `.onion` address as identity | Cryptographic node ID |
| PTT model (record → send) | Audio blob → CRDT document → mesh sync |
| App-layer encryption (independent of transport) | ChaCha20-Poly1305 (ADR-044) |
| Simple line protocol over Tor | peat-lite frame format over any transport (ADR-035) |
| Tor as anonymous transport | Noted as future transport possibility |
| `opusenc`/`opusdec` for audio | Opus codec (or Codec2 for constrained links) |

TerminalPhone validates the core concept: PTT voice as discrete encrypted messages over an overlay network. PEAT extends this to multi-transport mesh sync with CRDT persistence and transcription integration.

---

## Decision Drivers

### Requirements

1. **Audio as Blob**: Voice messages stored via ADR-025 BlobStore, referenced by BlobRef in CRDT documents
2. **Transport-Aware Encoding**: Audio quality adapts to available transport bandwidth
3. **PTT Model**: Record-then-send, not real-time streaming
4. **App-Layer Encryption**: Audio blobs encrypted independently of transport (ADR-044)
5. **Transcription Integration**: Link audio blobs to TranscriptionProduct for searchability
6. **Embedded Support**: Codec selection appropriate for resource-constrained devices (ADR-037)
7. **Multi-Transport**: Audio messages sync over any transport — QUIC, BLE, LoRa, SBD

### Constraints

1. **BLE throughput**: ~2 Mbps theoretical, practical ~100-200 KB/s — audio must be compact
2. **LoRa bandwidth**: 1.5-9.1 kB/s — only ultra-compressed codecs are viable
3. **SBD message size**: 1,960 bytes per message — fits ~22 seconds of Codec2 700C audio
4. **Embedded resources**: ESP32 has limited CPU/RAM — codec must be lightweight
5. **CRDT merge**: Binary blobs don't merge well in Automerge — must keep audio out of CRDT documents, using BlobRef indirection instead

---

## Decision

### Architecture: Audio-as-Blob Pattern

Voice messages follow the established blob-document integration pattern (ADR-025): audio payloads are stored in the BlobStore as content-addressed blobs, and CRDT documents hold only the metadata and BlobRef. This keeps audio out of the CRDT merge path while enabling mesh-wide discovery and sync.

```
┌──────────────────────────────────────────────────────────────┐
│  Sender Node                                                 │
│                                                              │
│  1. Record audio (microphone / PTT button)                   │
│  2. Encode: PCM → Opus (or Codec2 for constrained links)    │
│  3. Encrypt: ChaCha20-Poly1305 (ADR-044)                    │
│  4. Store blob: BlobStore::store_bytes(encrypted_audio)      │
│  5. Create AudioMessage document with BlobRef                │
│  6. Document + blob sync via mesh                            │
│                                                              │
└──────────────────────────────────────────────────────────────┘
                         │
                    mesh sync
                         │
┌──────────────────────────────────────────────────────────────┐
│  Receiver Node                                               │
│                                                              │
│  1. Receive AudioMessage document via CRDT sync              │
│  2. Extract BlobRef from document                            │
│  3. Fetch blob: BlobStore::fetch(blob_ref)                   │
│  4. Decrypt: ChaCha20-Poly1305                               │
│  5. Decode: Opus/Codec2 → PCM                                │
│  6. Play audio (speaker) or queue for playback               │
│                                                              │
│  Optional:                                                   │
│  7. Transcribe: Whisper/etc → TranscriptionProduct           │
│  8. Link transcription to AudioMessage document              │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### AudioMessage CRDT Document

Each voice message is represented by a single CRDT document (per ADR-021, one document per entity) containing metadata and a BlobRef to the encrypted audio payload:

```rust
/// AudioMessage CRDT document schema
///
/// Stored in the "audio_messages" collection.
/// Document ID: "{sender_node_id}:{timestamp_us}"
///
/// The audio payload itself is in the BlobStore — this document
/// holds only the metadata and BlobRef, keeping audio out of
/// the CRDT merge path.
pub struct AudioMessage {
    /// BlobRef to encrypted Opus/Codec2 audio in BlobStore
    pub audio_blob: BlobRef,

    /// Sender node ID (hex)
    pub sender_node_id: String,

    /// Recording timestamp (microseconds since epoch)
    pub recorded_at_us: u64,

    /// Audio duration in seconds
    pub duration_secs: f32,

    /// Codec information
    pub codec: AudioCodec,

    /// Optional: BlobRef to TranscriptionProduct
    pub transcription_blob: Option<BlobRef>,

    /// Optional: channel/group identifier for multi-channel PTT
    pub channel: Option<String>,

    /// TTL / expiry timestamp (microseconds since epoch, 0 = no expiry)
    pub expires_at_us: u64,
}

/// Audio codec metadata
pub struct AudioCodec {
    /// Codec name: "opus", "codec2"
    pub name: String,

    /// Bitrate in bits per second
    pub bitrate_bps: u32,

    /// Sample rate in Hz
    pub sample_rate_hz: u32,

    /// Number of channels (1 = mono, 2 = stereo)
    pub channels: u8,

    /// Codec2 mode (only for Codec2): "3200", "2400", "1600", "1300", "700C"
    pub codec2_mode: Option<String>,
}
```

### Transport-Aware Encoding

Audio quality adapts to the available transport, trading fidelity for bandwidth. The sender selects the codec and bitrate based on which transport will carry the message:

| Transport | Codec | Bitrate | Quality | Size per Minute | Notes |
|-----------|-------|---------|---------|-----------------|-------|
| QUIC/Iroh | Opus | 64 kbps stereo | Full quality | ~480 KB | Wideband, clear speech + ambient |
| QUIC/Iroh | Opus | 32 kbps mono | High quality | ~240 KB | Wideband, clear speech |
| BLE | Opus | 16 kbps mono | Good quality | ~120 KB | Narrowband, intelligible speech |
| LoRa (2.4 GHz) | Opus | 6 kbps mono | Acceptable | ~45 KB | Heavily compressed but intelligible |
| LoRa (868/915) | Codec2 2400 | 2.4 kbps | Low quality | ~18 KB | Robotic but intelligible speech |
| SBD | Codec2 700C | 700 bps | Minimal | ~5.25 KB | Fits ~22s in 1,960-byte SBD message |

**Codec selection logic:**

```rust
/// Select codec parameters based on transport bandwidth
pub fn select_audio_codec(transport_bandwidth_bps: u32) -> AudioCodec {
    match transport_bandwidth_bps {
        // QUIC/Iroh: full quality Opus
        bw if bw >= 100_000 => AudioCodec {
            name: "opus".into(),
            bitrate_bps: 64_000,
            sample_rate_hz: 48_000,
            channels: 2,
            codec2_mode: None,
        },
        // BLE: compact Opus mono
        bw if bw >= 20_000 => AudioCodec {
            name: "opus".into(),
            bitrate_bps: 16_000,
            sample_rate_hz: 16_000,
            channels: 1,
            codec2_mode: None,
        },
        // LoRa 2.4 GHz: minimal Opus
        bw if bw >= 6_000 => AudioCodec {
            name: "opus".into(),
            bitrate_bps: 6_000,
            sample_rate_hz: 8_000,
            channels: 1,
            codec2_mode: None,
        },
        // LoRa 868/915: Codec2 2400
        bw if bw >= 2_400 => AudioCodec {
            name: "codec2".into(),
            bitrate_bps: 2_400,
            sample_rate_hz: 8_000,
            channels: 1,
            codec2_mode: Some("2400".into()),
        },
        // SBD / extreme constraint: Codec2 700C
        _ => AudioCodec {
            name: "codec2".into(),
            bitrate_bps: 700,
            sample_rate_hz: 8_000,
            channels: 1,
            codec2_mode: Some("700C".into()),
        },
    }
}
```

**Codec data rate verification (from specifications):**

- **Opus**: RFC 6716. Supports 6 kbps – 510 kbps. At 16 kbps mono, produces ~120 KB/min (16,000 bits/s × 60s / 8 = 120,000 bytes). Narrowband mode (8 kHz) available down to 6 kbps.
- **Codec2**: Open-source speech codec by David Rowe VK5DGR. Mode 2400 produces 2,400 bits/s = 18 KB/min. Mode 700C produces 700 bits/s = 5.25 KB/min. At 700C, a 1,960-byte SBD message holds 1,960 × 8 / 700 ≈ 22.4 seconds of audio.

### Encryption

Audio blobs are encrypted at the application layer using ChaCha20-Poly1305, consistent with ADR-044 and ADR-006. Transport encryption (TLS for QUIC, link-layer for BLE/LoRa) is independent and additive:

```
┌─────────────────────────────────────────┐
│  Application Layer (this ADR)           │
│  ChaCha20-Poly1305 per blob             │
│  Key: per-channel or per-group PSK      │
│  Nonce: 12 bytes prepended to ciphertext│
│  Tag: 16 bytes appended                 │
├─────────────────────────────────────────┤
│  Transport Layer (independent)          │
│  QUIC: TLS 1.3                          │
│  BLE: AES-CCM (Bluetooth 4.2+)         │
│  LoRa: mLRS AES + app-layer (ADR-052)  │
│  SBD: Iridium link encryption           │
└─────────────────────────────────────────┘
```

Audio is always encrypted before being stored in the BlobStore. The BlobRef in the AudioMessage document is to the encrypted blob — receivers must hold the decryption key (channel PSK or group key) to play the audio.

### Transcription Integration

The existing `TranscriptionProduct` schema in `product.proto` provides speech-to-text with word-level timestamps and speaker diarization — a natural companion to audio blobs:

```protobuf
// Already defined in product.proto:
message TranscriptionProduct {
  string text = 1;              // Full transcribed text
  string language = 2;          // Language code (e.g., "en-US")
  float confidence = 3;         // Overall confidence
  float duration_seconds = 4;   // Audio duration
  repeated WordTimestamp words = 5;      // Word-level timing
  repeated SpeakerSegment speakers = 6;  // Speaker diarization
}
```

**Integration pattern:**

1. Node receives AudioMessage document with BlobRef
2. Node fetches and decrypts audio blob
3. Node runs speech-to-text (Whisper, etc.) on decoded audio
4. Node creates TranscriptionProduct and stores as blob
5. Node updates AudioMessage document with `transcription_blob` BlobRef
6. TranscriptionProduct syncs via CRDT — now the audio is text-searchable across the mesh

This is optional and compute-dependent: resource-constrained nodes skip transcription, while nodes with GPU/NPU capability transcribe and share results.

### Physical Device Concept ("Peat")

A purpose-built field audio device running PEAT for transport:

```
┌───────────────────────────────────────┐
│  "Peat" - PEAT Audio Field Device     │
│                                       │
│  ┌─────────┐  ┌────────────────────┐  │
│  │   Mic   │  │  Speaker           │  │
│  └────┬────┘  └────────┬───────────┘  │
│       │                │              │
│  ┌────▼────────────────▼───────────┐  │
│  │  Audio Pipeline                 │  │
│  │  PCM → Opus/Codec2 → Encrypt   │  │
│  │  Decrypt → Opus/Codec2 → PCM   │  │
│  └────────────┬────────────────────┘  │
│               │                       │
│  ┌────────────▼────────────────────┐  │
│  │  PEAT Node (peat-lite)          │  │
│  │  AudioMessage doc + BlobStore   │  │
│  └────────────┬────────────────────┘  │
│               │                       │
│  ┌────────────▼────────────────────┐  │
│  │  Transport(s)                   │  │
│  │  BLE │ LoRa │ WiFi/QUIC        │  │
│  └─────────────────────────────────┘  │
│                                       │
│  [PTT Button]  [Channel Dial]         │
│                                       │
│  Hardware: Pi Zero 2W or ESP32-S3     │
│  + I2S mic/speaker + LoRa SX1262      │
│  + BLE (built-in) + battery           │
└───────────────────────────────────────┘
```

The "Peat" concept demonstrates that PEAT can subsume the role of a tactical radio: audio captured locally, Opus-encoded, published as a CRDT blob, transported over whatever link is available. Unlike a traditional radio, the message persists, syncs to nodes that weren't online during transmission, and can be transcribed for searchability.

---

## Consequences

### Positive

- **Voice joins the data mesh**: Audio messages sync, persist, and are discoverable like any other CRDT data
- **Transport flexibility**: Voice travels over QUIC, BLE, LoRa, or SBD — whatever is available
- **Offline resilience**: PTT messages queue and sync when connectivity returns
- **Searchability**: Transcription integration makes voice content text-searchable
- **Consistent encryption**: Same app-layer encryption model as all other PEAT data (ADR-044)
- **Natural CRDT fit**: Immutable audio blobs + metadata documents work perfectly with existing blob-document integration (ADR-025)
- **Embedded viable**: Codec2 at 700 bps enables voice over SBD satellite — global voice messaging in < 2 KB

### Negative

- **Not real-time**: PTT latency is seconds to minutes, not suitable for conversation-paced dialogue
- **Codec complexity**: Supporting both Opus and Codec2 adds codec dependencies
- **Storage growth**: Audio blobs accumulate — need TTL/expiry and garbage collection
- **Transcription cost**: Speech-to-text requires significant compute (GPU/NPU) — not available on all nodes
- **Key management**: Per-channel encryption keys add key distribution complexity

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Audio blobs bloat storage | Medium | Medium | TTL/expiry on AudioMessage docs; BlobStore GC |
| Opus too heavy for ESP32 | Low | Medium | ESP32-S3 has sufficient CPU for Opus encode at 16 kbps; fallback to Codec2 |
| Codec2 quality insufficient | Medium | Low | 2400 mode is intelligible for speech; 700C is last resort for SBD only |
| Transcription accuracy on noisy field audio | High | Low | Confidence scores in TranscriptionProduct; human review for critical intel |
| Key distribution for channel PSKs | Medium | Medium | Reuse existing PEAT key distribution (ADR-044); pre-shared keys for initial deployment |

---

## Alternatives Considered

### Option 1: Real-Time Streaming Voice (WebRTC/RTP)
**Pros**: Natural conversation flow, low latency
**Cons**: Requires sustained low-latency transport (only QUIC), incompatible with store-and-forward transports (BLE, LoRa, SBD), doesn't persist, doesn't sync across disconnected nodes, fundamentally different problem than CRDT data sync
**Decision**: Deferred — streaming voice is a separate feature that could coexist alongside PTT, but requires dedicated real-time transport infrastructure that PEAT doesn't currently have.

### Option 2: Integrate TerminalPhone Directly
**Pros**: Working implementation, proven concept
**Cons**: Written in Bash (not embeddable in Rust/embedded), depends on Tor (not available on embedded), tightly coupled to Unix toolchain (`opusenc`, `sox`, `socat`), single transport only
**Decision**: Rejected as direct integration. TerminalPhone validates the concept but PEAT needs a native Rust implementation that works across all transports and on embedded platforms.

### Option 3: Tor as a PEAT Transport
**Pros**: Anonymous communication, TerminalPhone compatibility, censorship resistance
**Cons**: High latency (seconds), requires Tor daemon, not available on embedded, adds significant complexity
**Decision**: Noted as future possibility. Tor could be a transport plugin (ADR-032) independent of this audio ADR. The audio architecture is transport-agnostic by design.

### Option 4: Audio Directly in CRDT Documents (Not Blob)
**Pros**: Simpler — no blob indirection
**Cons**: Automerge handles binary data inefficiently for merge operations; a 20 KB audio clip embedded in a document would be treated as an opaque binary field that can't merge, creating conflicts on concurrent updates to other fields in the same document; defeats CRDT delta efficiency (ADR-021)
**Decision**: Rejected — BlobRef indirection (ADR-025) is the established pattern for binary data. Audio payloads in the BlobStore, metadata in the CRDT document.

---

## Related ADRs

| ADR | Relationship |
|-----|-------------|
| ADR-021 (Document-Oriented Architecture) | AudioMessage follows one-document-per-entity pattern |
| ADR-025 (Blob Transfer Protocol) | Audio stored as content-addressed blobs via BlobStore trait |
| ADR-032 (Transport Abstraction) | Transport bandwidth drives codec selection |
| ADR-035 (peat-lite Embedded Nodes) | Embedded node constraints for audio on ESP32 |
| ADR-037 (Resource-Constrained Devices) | Codec2 selection for constrained nodes |
| ADR-039 (BLE Transport) | BLE bandwidth constraints for audio quality |
| ADR-044 (Encryption) | ChaCha20-Poly1305 app-layer encryption for audio blobs |
| ADR-051 (SBD Satellite Transport) | Codec2 700C enables voice over 1,960-byte SBD messages |
| ADR-052 (LoRa Transport) | LoRa bandwidth constraints; Codec2 for 868/915 MHz bands |

---

## References

1. [TerminalPhone](https://gitlab.com/here_forawhile/terminalphone) — Anonymous encrypted PTT voice over Tor hidden services (Bash)
2. [Opus Codec (RFC 6716)](https://www.rfc-editor.org/rfc/rfc6716) — Versatile audio codec, 6-510 kbps, open standard
3. [Codec2](https://www.rowetel.com/?page_id=452) — Open-source speech codec by David Rowe VK5DGR, 700-3200 bps
4. [opus-rs](https://crates.io/crates/opus) — Rust bindings for libopus
5. [codec2-rs](https://crates.io/crates/codec2) — Rust bindings for Codec2
6. ADR-021: Document-Oriented Architecture and Update Semantics
7. ADR-025: Blob Transfer Protocol
8. ADR-032: Pluggable Transport Abstraction
9. ADR-035: PEAT-Lite Embedded Nodes (peat-lite protocol)
10. ADR-044: Application-Layer Encryption
11. ADR-051: PEAT-SBD Satellite Transport
12. ADR-052: PEAT-LoRa Long-Range Radio Transport
13. `peat-schema/proto/product.proto` — TranscriptionProduct definition (speech-to-text with word timestamps and speaker diarization)
14. `peat-protocol/src/storage/blob_document_integration.rs` — BlobDocumentIntegration trait and BlobReference types

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-02-26 | Proposed ADR-053 | Voice is a natural fit for CRDT blob sync; TerminalPhone validates the PTT-over-overlay concept |
| 2026-02-26 | PTT model, not streaming | Record-then-send maps to CRDT sync; streaming requires fundamentally different transport |
| 2026-02-26 | Audio-as-Blob via ADR-025 | Binary audio payloads don't belong in CRDT merge path; BlobRef indirection is established pattern |
| 2026-02-26 | Transport-aware codec selection | Opus for high-bandwidth (QUIC, BLE), Codec2 for extreme constraint (LoRa, SBD) |
| 2026-02-26 | App-layer encryption per ADR-044 | Audio encrypted before BlobStore; transport encryption is independent/additive |
| 2026-02-26 | Transcription via existing TranscriptionProduct | product.proto already has speech-to-text schema with word timestamps and diarization |

---

**Next Steps:**
1. Review and approve ADR
2. Prototype AudioMessage document schema in peat-schema
3. Prototype Opus encoding pipeline (Rust, using `opus` crate)
4. Prototype Codec2 encoding for SBD/LoRa constraint testing
5. Build "Peat" hardware prototype (Pi Zero 2W + I2S mic/speaker + SX1262)
6. Integration test: record PTT → encode → encrypt → blob store → CRDT sync → decrypt → decode → playback
7. Transcription integration: audio blob → Whisper → TranscriptionProduct → link to AudioMessage
