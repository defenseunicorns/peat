# Peat Protocol Examples

This directory contains normative message examples for the Peat Protocol.

## Purpose

These examples demonstrate correct message formation and protocol flows. Implementations SHOULD use these examples as test vectors for conformance validation.

## Planned Examples

### Discovery Phase

- [ ] `beacon-basic.json` - Minimal valid beacon
- [ ] `beacon-full.json` - Beacon with all optional fields
- [ ] `beacon-query.json` - Discovery query examples

### Cell Formation

- [ ] `cell-formation-request.json` - Join request
- [ ] `cell-formation-response.json` - Join response
- [ ] `cell-leader-election.json` - Leader election scenario

### Hierarchical Operations

- [ ] `squad-summary.json` - Squad aggregation
- [ ] `platoon-summary.json` - Platoon aggregation
- [ ] `command-flow.json` - Command and acknowledgment

### Composition

- [ ] `composition-additive.json` - Additive composition
- [ ] `composition-emergent.json` - Emergent capability discovery
- [ ] `composition-redundant.json` - Redundant composition
- [ ] `composition-constraint.json` - Constraint-based composition

## Format

Examples are provided in JSON (human-readable) format. Implementations should verify they can:

1. Parse the JSON
2. Convert to Protocol Buffer binary
3. Deserialize back to JSON
4. Verify semantic equivalence

## Contributing

To add examples:

1. Create JSON file in appropriate subdirectory
2. Include comments explaining the example
3. Verify example passes schema validation
4. Add entry to this README
