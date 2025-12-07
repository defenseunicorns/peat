# HIVE Object Tracking - Data Flow Sequence

This diagram shows the message flow for the object tracking vignette, including the MLOps model update flow.

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                                  │
│   PHASE 1: INITIALIZATION & CAPABILITY ADVERTISEMENT                                                            │
│                                                                                                                  │
│   Team Alpha                      Coordinator                         C2 Element                                │
│   ─────────────                   ───────────                         ──────────                                │
│                                                                                                                  │
│   Alpha-1,2,3 ──────► HIVE Discovery                                                                            │
│   (team forms)                    │                                                                              │
│        │                          │                                                                              │
│        ├──────────────────────────┤                                                                              │
│        │  HIVE: Capability        │                                                                              │
│        │  Advertisement           │                                                                              │
│        │  • Camera: operational   │                                                                              │
│        │  • AI Model: YOLOv8 v1.2 │                                                                              │
│        │    precision=0.91        │                                                                              │
│        │                          │                                                                              │
│   (same for Team Bravo)           ├──────────────────────────────────►│                                          │
│                                   │ CoT: Register Formation           │                                          │
│                                   │ (MIL-STD-2525 symbol)             │                                          │
│                                   │                                   │ ◄──── WebTAK shows                       │
│                                   │                                   │        platoon on map                    │
│                                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                                  │
│   PHASE 2: MISSION TASKING                                                                                       │
│                                                                                                                  │
│   Team Alpha                      Coordinator                         C2 Element                                │
│   ─────────────                   ───────────                         ──────────                                │
│                                                                                                                  │
│        │                          │                                   │                                          │
│        │                          │ ◄─────────────────────────────────┤                                          │
│        │                          │ CoT: <t-x-m> Mission Task         │ Commander creates                        │
│        │                          │   target: "Blue jacket, backpack" │ "Track POI" in WebTAK                   │
│        │                          │   boundary: [polygon]             │                                          │
│        │                          │                                   │                                          │
│        │ ◄────────────────────────┤                                   │                                          │
│        │ HIVE: TRACK_TARGET       │ Converts CoT → HIVE Command       │                                          │
│        │   command                │                                   │                                          │
│        │                          │                                   │                                          │
│   Alpha-1 (ATAK)                  │                                   │                                          │
│   displays mission                │                                   │                                          │
│                                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                                  │
│   PHASE 3: ACTIVE TRACKING                                                                                       │
│                                                                                                                  │
│   Alpha-2 (UGV)     Alpha-3 (AI)            Coordinator                         C2 Element                      │
│   ─────────────     ──────────              ───────────                         ──────────                      │
│                                                                                                                  │
│   Camera ──────────► YOLOv8 v1.2                                                                                 │
│   frames             │                                                                                           │
│   (5 Mbps)           │ Detect + Track                                                                            │
│                      │                                                                                           │
│                      ├─────────────────────►│                                                                    │
│                      │ HIVE: TrackUpdate    │                                                                    │
│                      │   track_id: TRACK-001│                                                                    │
│                      │   position: lat/lon  │                                                                    │
│                      │   confidence: 0.89   │                                                                    │
│                      │   model_ver: 1.2.0   │                                                                    │
│                      │   (~500 bytes)       │                                                                    │
│                      │                      │                                                                    │
│                      │                      ├─────────────────────────────────►│                                 │
│                      │                      │ CoT: Position Event              │                                 │
│                      │                      │   uid: TRACK-001                 │ Track appears                   │
│                      │                      │   type: a-f-G-E-S                │ on WebTAK map                   │
│                      │                      │                                  │                                 │
│                                                                                                                  │
│   ╔════════════════════════════════════════════════════════════════════════════════════════════════════════════╗ │
│   ║  BANDWIDTH COMPARISON (Tracking Only)                                                                      ║ │
│   ║  ────────────────────────────────────                                                                      ║ │
│   ║                                                                                                            ║ │
│   ║  Traditional Approach:     │████████████████████████████████████████████│  5 Mbps (video stream)          ║ │
│   ║                                                                                                            ║ │
│   ║  HIVE Approach:            │█│  ~1 Kbps (track updates @ 2 Hz)                                             ║ │
│   ║                                                                                                            ║ │
│   ║  REDUCTION: 99.98%                                                                                         ║ │
│   ╚════════════════════════════════════════════════════════════════════════════════════════════════════════════╝ │
│                                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                                  │
│   PHASE 4: TRACK HANDOFF                                                                                         │
│                                                                                                                  │
│   Team Alpha              Coordinator              Team Bravo                  C2 Element                       │
│   ─────────────           ───────────              ───────────                 ──────────                       │
│                                                                                                                  │
│   POI approaching         │                                                                                      │
│   boundary ────────────►  │                                                                                      │
│                           │                                                                                      │
│                           │ ───────────────────────►│                                                            │
│                           │ HIVE: PREPARE_HANDOFF   │                                                            │
│                           │   track_history: [...]  │                                                            │
│                           │   poi_description: ...  │                                                            │
│                           │                         │                                                            │
│                           │                         │ Bravo-2 repositions                                        │
│                           │                         │ Bravo-3 searches...                                        │
│                           │                         │                                                            │
│                           │ ◄───────────────────────┤                                                            │
│                           │ HIVE: TRACK_ACQUIRED    │                                                            │
│                           │   new_track: TRACK-002  │                                                            │
│                           │   confidence: 0.91      │                                                            │
│                           │                         │                                                            │
│   ◄───────────────────────┤                         │                                                            │
│   HIVE: HANDOFF_COMPLETE  │ Correlates:            │                                                            │
│                           │ TRACK-001 == TRACK-002 │                                                            │
│                           │                         │                                                            │
│   Alpha-3: status =       │ ───────────────────────────────────────────────►│                                   │
│   HANDED_OFF              │ CoT: Track continues                             │ Unified track                    │
│                           │   (same uid: TRACK-001)                          │ uninterrupted                    │
│                                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                                  │
│   PHASE 5: MLOps - MODEL UPDATE DISTRIBUTION                                                                     │
│                                                                                                                  │
│   MLOps Server           TAK Server         Coordinator         Team Alpha         Team Bravo                   │
│   ────────────           ──────────         ───────────         ───────────        ───────────                  │
│                                                                                                                  │
│   C2 observes low-light                                                                                          │
│   confidence drop...                                                                                             │
│        │                                                                                                         │
│   Retrained model ready                                                                                          │
│   YOLOv8 v1.3.0                                                                                                  │
│   (low-light improved)                                                                                           │
│        │                                                                                                         │
│        ├────────────────►│                                                                                       │
│        │ Model Package   │                                                                                       │
│        │ {               │                                                                                       │
│        │   version:1.3.0 │                                                                                       │
│        │   hash: sha256  │                                                                                       │
│        │   size: 45MB    │                                                                                       │
│        │   targets: [*]  │                                                                                       │
│        │ }               │                                                                                       │
│        │                 │                                                                                       │
│        │                 ├─────────────────►│                                                                    │
│        │                 │ HIVE: MODEL_PKG  │                                                                    │
│        │                 │ (via TAK Bridge) │                                                                    │
│        │                 │                  │                                                                    │
│        │                 │                  │ ════════════════════════════════════════════════                   │
│        │                 │                  │   DOWNWARD FLOW (Coordinator → Teams)                             │
│        │                 │                  │   Priority: P5 (Bulk) - Background transfer                       │
│        │                 │                  │   Content-addressed blob (only delta if cached)                   │
│        │                 │                  │ ════════════════════════════════════════════════                   │
│        │                 │                  │                                                                    │
│        │                 │                  ├────────────────────►│                                              │
│        │                 │                  │ HIVE: MODEL_UPDATE  │                                              │
│        │                 │                  │   blob_ref: sha256  │                                              │
│        │                 │                  │                     │                                              │
│        │                 │                  │                     │ Alpha-3 receives                             │
│        │                 │                  │                     │ Verifies hash ✓                              │
│        │                 │                  │                     │ Hot-swaps model                              │
│        │                 │                  │                     │ (~2 sec pause)                               │
│        │                 │                  │                     │                                              │
│        │                 │                  │ ◄────────────────────┤                                              │
│        │                 │                  │ HIVE: Capability    │                                              │
│        │                 │                  │ Re-Advertisement    │                                              │
│        │                 │                  │   model_ver: 1.3.0  │                                              │
│        │                 │                  │   precision: 0.94   │                                              │
│        │                 │                  │                     │                                              │
│        │                 │                  ├───────────────────────────────────────►│                           │
│        │                 │                  │ HIVE: MODEL_UPDATE                     │                           │
│        │                 │                  │   (same blob_ref)                      │                           │
│        │                 │                  │                                        │                           │
│        │                 │                  │                                        │ Bravo-3 receives          │
│        │                 │                  │                                        │ Verifies hash ✓           │
│        │                 │                  │                                        │ Hot-swaps model           │
│        │                 │                  │                                        │                           │
│        │                 │                  │ ◄──────────────────────────────────────┤                           │
│        │                 │                  │ HIVE: Capability Re-Advertisement      │                           │
│        │                 │                  │   model_ver: 1.3.0                     │                           │
│        │                 │                  │   precision: 0.94                      │                           │
│        │                 │                  │                                                                    │
│        │                 │                  │ Aggregates: All platforms now v1.3.0                               │
│        │                 │                  │                                                                    │
│        │                 │ ◄────────────────┤                                                                    │
│        │                 │ CoT: Status      │                                                                    │
│        │                 │ "Model update    │                                                                    │
│        │ ◄───────────────│  complete"       │                                                                    │
│        │                 │                  │                                                                    │
│   WebTAK: Commander sees                                                                                         │
│   "All platforms updated                                                                                         │
│    to v1.3.0"                                                                                                    │
│                                                                                                                  │
│   ╔════════════════════════════════════════════════════════════════════════════════════════════════════════════╗ │
│   ║  MODEL UPDATE CHARACTERISTICS                                                                              ║ │
│   ║  ────────────────────────────                                                                              ║ │
│   ║                                                                                                            ║ │
│   ║  Transfer Size:       45 MB (compressed ONNX model)                                                        ║ │
│   ║  Priority:            P5 (Bulk) - Does NOT interrupt active tracking                                       ║ │
│   ║  Verification:        SHA256 hash check before deployment                                                  ║ │
│   ║  Deployment:          Rolling (one platform at a time)                                                     ║ │
│   ║  Tracking Pause:      ~2 seconds per platform during hot-swap                                              ║ │
│   ║  Rollback Ready:      v1.2.0 cached locally on each platform                                               ║ │
│   ║                                                                                                            ║ │
│   ╚════════════════════════════════════════════════════════════════════════════════════════════════════════════╝ │
│                                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                                                  │
│   PHASE 6: MISSION COMPLETE                                                                                      │
│                                                                                                                  │
│   Team Bravo              Coordinator                                   C2 Element                              │
│   ─────────────           ───────────                                   ──────────                              │
│                                                                                                                  │
│   POI exits boundary      │                                                                                      │
│   (with improved v1.3.0   │                                                                                      │
│    tracking)              │                                                                                      │
│   ────────────────────►   │                                                                                      │
│                           │                                                                                      │
│   Bravo-3: status =       │ ────────────────────────────────────────────►│                                      │
│   EXITED_AOI              │ CoT: Track Complete                          │                                      │
│                           │   status: "exited_aoi"                       │ Mission logged                       │
│                           │                                              │                                      │
│                           │ MISSION SUMMARY:                             │                                      │
│                           │ ╔════════════════════════════════════════╗   │                                      │
│                           │ ║ Track Duration:      25 minutes        ║   │                                      │
│                           │ ║ Handoffs:            1 (Alpha→Bravo)   ║   │                                      │
│                           │ ║ Track Continuity:    100%              ║   │                                      │
│                           │ ║ Avg Confidence:      0.88              ║   │                                      │
│                           │ ║ Model Updates:       1 (v1.2→v1.3)     ║   │                                      │
│                           │ ║                                        ║   │                                      │
│                           │ ║ Data Transmitted:                      ║   │                                      │
│                           │ ║   Track Updates:     47 KB             ║   │                                      │
│                           │ ║   Model Update:      45 MB             ║   │                                      │
│                           │ ║   Total:             ~45 MB            ║   │                                      │
│                           │ ║                                        ║   │                                      │
│                           │ ║ Traditional Would Have Used:           ║   │                                      │
│                           │ ║   Video Streams:     938 MB            ║   │                                      │
│                           │ ║   (25 min × 2 cam × 5 Mbps)           ║   │                                      │
│                           │ ║                                        ║   │                                      │
│                           │ ║ HIVE SAVINGS:        95%               ║   │                                      │
│                           │ ║ (Even with full model push)            ║   │                                      │
│                           │ ╚════════════════════════════════════════╝   │                                      │
│                                                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Bidirectional Flow Summary

```
                           ┌─────────────────────────────────────┐
                           │           C2 / MLOps                │
                           └──────────────┬──────────────────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
                    ▼                     │                     ▼
         ╔══════════════════╗             │          ╔══════════════════╗
         ║   UPWARD FLOW    ║             │          ║  DOWNWARD FLOW   ║
         ║   (Decisions)    ║             │          ║  (Capabilities)  ║
         ╠══════════════════╣             │          ╠══════════════════╣
         ║ • Track updates  ║             │          ║ • Model packages ║
         ║ • Capability ads ║             │          ║ • Commands       ║
         ║ • Health status  ║             │          ║ • Configuration  ║
         ║ • Handoff events ║             │          ║ • Geofences      ║
         ╚══════════════════╝             │          ╚══════════════════╝
                    │                     │                     │
                    │              ┌──────┴──────┐              │
                    │              │ Coordinator │              │
                    │              │  (Bridge)   │              │
                    │              └──────┬──────┘              │
                    │                     │                     │
                    └─────────────────────┼─────────────────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
                    ▼                     ▼                     ▼
              ┌──────────┐         ┌──────────┐          ┌──────────┐
              │  Team A  │         │  Team B  │          │  Team N  │
              │  (Edge)  │         │  (Edge)  │          │  (Edge)  │
              └──────────┘         └──────────┘          └──────────┘
```

## Key HIVE Protocol Messages

### Track Update (HIVE → Coordinator → C2)
```json
{
  "track_id": "TRACK-001",
  "classification": "person",
  "confidence": 0.89,
  "position": { "lat": 33.7749, "lon": -84.3958, "cep_m": 2.5 },
  "velocity": { "bearing": 45, "speed_mps": 1.2 },
  "attributes": { "jacket_color": "blue", "has_backpack": true },
  "source_platform": "Alpha-2",
  "source_model": "Alpha-3",
  "model_version": "1.3.0",
  "timestamp": "2025-11-26T14:10:00Z"
}
```

### Capability Advertisement (Team → Coordinator)
```json
{
  "platform_id": "Alpha-3",
  "advertised_at": "2025-11-26T14:25:00Z",
  "models": [{
    "model_id": "object_tracker",
    "model_version": "1.3.0",
    "model_hash": "sha256:b8d9c4e2f1a3...",
    "model_type": "detector_tracker",
    "performance": { "precision": 0.94, "recall": 0.89, "fps": 15 },
    "operational_status": "READY"
  }]
}
```

### Model Update Package (C2 → Coordinator → Teams)
```json
{
  "package_type": "AI_MODEL_UPDATE",
  "model_id": "object_tracker",
  "model_version": "1.3.0",
  "model_hash": "sha256:b8d9c4e2f1a3...",
  "model_size_bytes": 45000000,
  "blob_reference": "hive://blobs/sha256:b8d9c4e2f1a3...",
  "target_platforms": ["Alpha-3", "Bravo-3"],
  "deployment_policy": "ROLLING",
  "rollback_version": "1.2.0",
  "metadata": {
    "changelog": "Improved low-light detection",
    "training_date": "2025-11-26",
    "validation_accuracy": 0.94
  }
}
```

### CoT Track Event (Coordinator → TAK Server)
```xml
<event uid="TRACK-001" type="a-f-G-E-S" time="2025-11-26T14:10:00Z"
       start="2025-11-26T14:10:00Z" stale="2025-11-26T14:15:00Z" how="m-g">
  <point lat="33.7749" lon="-84.3958" hae="0" ce="2.5" le="1"/>
  <detail>
    <track course="45" speed="1.2"/>
    <remarks>person - Blue jacket, backpack (89% confidence)</remarks>
    <_hive_ model_version="1.3.0"/>
  </detail>
</event>
```
