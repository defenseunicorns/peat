# Peat Protocol: Core Positioning Guide

**Organization:** Defense Unicorns  
**URL:** https://defenseunicorns.com  
**Document Purpose:** Authoritative framing for all Peat communications  
**Last Updated:** December 2024

---

## Executive Summary

Peat is a **coordination protocol for human-machine-AI teams** that enables continuous decision-making superiority across the full system-of-systems stack—from on-body sensors to coalition interfaces.

Peat is **not** a swarm control system. It is **not** primarily about unmanned platforms. It is the coordination fabric that enables organizations to decide faster and better than adversaries across all echelons and all team members—human, machine, and AI alike.

---

## Core Value Proposition

**"Stop moving data, start moving decisions."**

Peat enables distributed decision-making through hierarchical capability aggregation. Rather than fusing raw data at a central node, Peat synthesizes what teams CAN DO and what commanders NEED TO KNOW at each echelon—matching how human organizations actually make decisions.

---

## What Peat Is

- **A coordination fabric** for human-machine-AI teams operating as integrated units
- **A decision-support architecture** that aggregates capabilities and synthesizes situational awareness
- **A system-of-systems protocol** spanning on-body wearables through coalition interfaces
- **An enabler of continuous operations** under contested, degraded, and denied conditions
- **Infrastructure** that makes existing standards (TAK, Link 16, STANAG 4586, ROS2) work at scale

## What Peat Is Not

- ❌ A swarm control system for unmanned platforms
- ❌ A replacement for existing C2 systems or data links
- ❌ A sensor fusion or data aggregation platform
- ❌ A "1000+ drone" coordination solution
- ❌ Platform-centric autonomy software

---

## The Full Stack

Peat addresses coordination across the complete operational hierarchy:

```
┌─────────────────────────────────────────┐
│  Cross-Organizational / Coalition       │  ← Interoperability, shared awareness
├─────────────────────────────────────────┤
│  Theater / Division                     │  ← Strategic intent distribution
├─────────────────────────────────────────┤
│  Battalion / Company                    │  ← Operational coordination
├─────────────────────────────────────────┤
│  Platoon / Squad                        │  ← Tactical integration
├─────────────────────────────────────────┤
│  Individual Warfighter                  │  ← Human as team member
├─────────────────────────────────────────┤
│  On-Human (Wearables, WearTAK)          │  ← Edge sensing, local AI
├─────────────────────────────────────────┤
│  Sub-Tier (Embedded AI, Micro-sensors)  │  ← Continuous capability feed
└─────────────────────────────────────────┘
```

Every node—human, machine, or AI—participates as a first-class team member with:
- **Capabilities** (what can I contribute?)
- **State** (what is my current status?)
- **Authority** (what am I permitted to decide?)

---

## Humans as Team Members, Not Operators

**Critical distinction from swarm programs (OFFSET, AMASS):**

| Swarm Paradigm | Peat Paradigm |
|----------------|---------------|
| Humans *command* machines | Humans *coordinate with* machines and AI |
| Operator above the loop | Team member in the loop |
| Platform count as metric | Decision quality as metric |
| Automation of tasks | Integration of capabilities |
| Machine autonomy levels | Team authority gradients |

Peat models human cognitive load, fatigue, authority, and decision-making capacity as core protocol elements—not afterthoughts. The warfighter's state matters as much as any sensor platform's state because **the human is part of the team, not external to it.**

---

## Decision-Making Superiority

Peat's purpose is enabling **continuous decision-making superiority**—the ability to:

1. **Understand faster** — Hierarchical aggregation surfaces relevant capabilities and threats without drowning commanders in raw data
2. **Decide faster** — Local authority enables immediate response within intent boundaries
3. **Adapt faster** — CRDT-based synchronization heals network partitions automatically, maintaining coordination through degradation
4. **Scale without penalty** — O(n log n) complexity means adding nodes improves capability without overwhelming communications

### The Three Flows

**Capability Aggregation (Up):**  
"What can my team do right now?" — Synthesized at each echelon, not enumerated

**Intent Distribution (Down):**  
"What should we accomplish?" — Commander's intent flows to enable local decisions

**Peer Coordination (Lateral):**  
"What are adjacent teams doing?" — Boundary coordination without central mediation

---

## Why Hierarchy?

Military hierarchy is not bureaucratic overhead—it is **evolved communication optimization**.

- **Flat mesh:** O(n²) message complexity → fails at ~20 nodes
- **Hierarchy:** O(n log n) complexity → scales to thousands

This mirrors how human organizations actually function. A battalion commander doesn't track every soldier's position—they understand company capabilities and dispositions. Peat encodes this natural pattern into protocol.

---

## Comparison to Related Programs

### DARPA OFFSET (Completed 2021)
- **Focus:** Swarm tactics, human-swarm interfaces for ~250 UAS/UGS
- **Gap:** No coordination architecture, no scaling solution, platform-centric
- **Peat relationship:** OFFSET trains behaviors; Peat could coordinate those behaviors within broader human-machine-AI teams

### DARPA AMASS (Active)
- **Focus:** Theatre-level C2 language for 1000+ unmanned platforms
- **Gap:** Assumes coordination infrastructure exists; still platform-centric
- **Peat relationship:** Peat provides the coordination layer that makes AMASS-style theatre C2 actually function at scale

### DARPA TIAMAT (Active)
- **Focus:** Sim-to-real transfer for individual platform behaviors
- **Gap:** Single-platform autonomy, assumes coordination is solved
- **Peat relationship:** TIAMAT-trained platforms could use Peat for team coordination

### DIU Common Operational Database (COD)
- **Focus:** Event-streaming for multi-platform awareness
- **Failure mode:** O(n²) scaling collapsed at ~20 platforms
- **Peat relationship:** Peat solves the exact scaling problem that caused COD to fail

**Key insight:** These programs focus on platforms. Peat focuses on teams. The human-machine-AI team is the unit of action—not the individual drone, robot, or soldier.

---

## Technical Differentiators

| Attribute | Traditional Approaches | Peat |
|-----------|----------------------|------|
| Synchronization | Event streaming, polling | CRDT-based eventual consistency |
| Scaling | O(n²) mesh | O(n log n) hierarchy |
| Bandwidth | 100% state transmission | 95-99% reduction via aggregation |
| Failure handling | Central point of failure | Partition-tolerant, self-healing |
| Human integration | Operator interface | First-class team member |
| AI integration | Tool/automation | Capability contributor |

---

## Messaging Framework

### For Autonomy Program Managers
"Peat enables your autonomous systems to coordinate with human teams at scale—not just with each other."

### For Military Doctrine Specialists  
"Peat encodes doctrinal command relationships into protocol, making hierarchy an asset rather than a bottleneck."

### For Data/AI Architects
"Peat moves decisions instead of data, enabling edge AI to contribute to team awareness without saturating tactical networks."

### For Acquisition Officials
"Peat is open-source infrastructure (Apache 2.0) that makes your existing investments in TAK, Link 16, and autonomous systems work together at scale."

### For Coalition Partners
"Peat enables interoperability without vendor lock-in, providing a coordination layer that respects national system boundaries while enabling combined operations."

---

## Prohibited Framings

When communicating about Peat, **avoid** these framings that limit or misrepresent the protocol:

- ❌ "Drone swarm coordination"
- ❌ "Alternative to [specific platform C2 system]"
- ❌ "1000+ platform control"
- ❌ "Unmanned system protocol"
- ❌ "Data fusion system"
- ❌ "Sensor aggregation"
- ❌ Platform counts as primary metric

**Instead, emphasize:**

- ✅ Human-machine-AI team coordination
- ✅ Decision-making superiority
- ✅ System-of-systems fabric
- ✅ Full-stack (on-body → coalition)
- ✅ Continuous operations
- ✅ Capability aggregation
- ✅ Intent distribution

---

## Standard Boilerplate

### One-Sentence
Peat is an open-source coordination protocol that enables human-machine-AI teams to achieve continuous decision-making superiority across all echelons—from on-body sensors to coalition interfaces.

### One-Paragraph
Peat Protocol solves the fundamental coordination challenge facing modern military operations: how to integrate humans, machines, and AI into effective teams that can decide faster than adversaries. Unlike swarm control systems focused on unmanned platforms, Peat treats every participant—warfighter, autonomous system, edge AI—as a first-class team member contributing capabilities to shared awareness. Through hierarchical CRDT-based synchronization, Peat achieves 95-99% bandwidth reduction while maintaining decision-quality under contested, degraded, and denied conditions. The result: organizations that think and act as integrated wholes, not collections of disconnected systems.

### Technical Summary
Peat implements hierarchical capability aggregation using Conflict-free Replicated Data Types (CRDTs) to achieve O(n log n) coordination complexity across human-machine-AI teams. The protocol separates capability advertisement, intent distribution, and peer coordination into distinct but synchronized flows, enabling local autonomy within commander's intent while maintaining global coherence. Peat integrates with existing standards (TAK/CoT, STANAG 4586, Link 16, ROS2) as infrastructure rather than replacement, solving the scaling barrier that limits current approaches to approximately 20 coordinated platforms.

---

## Document Control

This positioning guide should inform all Peat communications including:
- Pitch decks and investor materials
- Proposal abstracts and technical volumes
- Website and marketing content
- Conference presentations
- Academic publications
- Partner discussions

Updates to this document require review to ensure consistency across all project materials.

---

**Defense Unicorns**  
https://defenseunicorns.com
