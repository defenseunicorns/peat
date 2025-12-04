# HIVE Protocol Diagrams

This directory contains normative diagrams for the HIVE Protocol specification.

## Purpose

These diagrams illustrate protocol concepts, message flows, and architectural patterns. They are referenced from the main specification document.

## Planned Diagrams

### Architecture

- [ ] `three-phase-overview.svg` - Protocol phase diagram
- [ ] `hierarchy-structure.svg` - Squad → Platoon → Company
- [ ] `data-flow.svg` - Upward aggregation, downward commands

### Message Flows

- [ ] `discovery-flow.svg` - Beacon exchange sequence
- [ ] `cell-formation-flow.svg` - Cell join protocol
- [ ] `command-dissemination.svg` - Command and ack flow

### CRDT Semantics

- [ ] `crdt-types.svg` - CRDT type usage diagram
- [ ] `conflict-resolution.svg` - LWW merge example

### Composition

- [ ] `composition-patterns.svg` - Four composition types
- [ ] `emergent-capability.svg` - ISR chain example

## Format

Diagrams should be provided as:

- **SVG** (preferred) - Scalable, editable
- **PNG** - For compatibility (300 DPI minimum)

Source files (if applicable):

- Mermaid markdown
- PlantUML
- Draw.io XML

## Contributing

To add diagrams:

1. Create source file and rendered output
2. Use consistent styling (colors, fonts)
3. Include in specification with figure reference
4. Add entry to this README
