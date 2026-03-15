# ADR-028: Peat CoT Custom Detail Extension Schema

**Status**: Proposed
**Date**: 2025-11-26
**Authors**: Kit Plummer
**Related ADRs**:
- [ADR-020](020-TAK-CoT-Integration.md) (TAK & CoT Integration)
- [ADR-012](012-schema-definition-protocol-extensibility.md) (Schema Definition & Protocol Extensibility)
- [ADR-019](019-qos-and-data-prioritization.md) (QoS and Data Prioritization)

**Source**: M1 POC integrator feedback (TAK_INTEGRATION_REQUIREMENTS.md)

## Context

### Problem Statement

When translating Peat messages to Cursor-on-Target (CoT) XML format for TAK integration, significant semantic information is lost. CoT's core schema supports:
- Position (lat/lon/hae)
- Identity (uid, type)
- Time bounds (time, start, stale)
- Basic details (remarks, links, contacts)

However, Peat messages contain rich context that TAK operators need:
- **Source attribution**: Which platform and AI model produced this data?
- **Confidence scores**: How reliable is this track detection?
- **Hierarchy membership**: Which cell/formation does this belong to?
- **Capability status**: What can this platform do? Is it degraded?
- **Custom attributes**: Domain-specific metadata (clothing color, vehicle type, etc.)

Without preserving this information, TAK operators cannot make informed decisions about Peat-coordinated assets.

### CoT Extensibility

CoT supports custom detail elements via XML namespaces. Elements starting with `_` are treated as extensions and passed through by TAK servers/clients that don't recognize them. This allows Peat to embed rich metadata while maintaining compatibility.

**TAK Extension Convention**:
- Element names starting with `_` are extensions
- Extensions should use XML namespaces for disambiguation
- TAK passes through unknown extensions without modification
- ATAK plugins can render custom extensions

## Decision

We will define a standardized `<_peat_>` CoT detail extension schema for embedding Peat-specific semantics in CoT messages.

### Schema Definition

**Namespace**: `urn:peat:cot:1.0`
**Element Name**: `_peat_`
**Version**: `1.0`

```xml
<_peat_ version="1.0" xmlns:peat="urn:peat:cot:1.0">
  <!-- Source Attribution -->
  <source platform="{platform_id}"
          model="{model_id}"
          model_version="{version}"
          model_hash="{optional_hash}"/>

  <!-- Track Confidence (for TrackUpdate messages) -->
  <confidence value="{0.0-1.0}"
              threshold="{optional_threshold}"/>

  <!-- Hierarchy Membership -->
  <hierarchy>
    <cell id="{cell_id}" role="{role}"/>
    <formation id="{formation_id}"/>
    <zone id="{zone_id}"/>
  </hierarchy>

  <!-- Custom Attributes (pass-through from Peat messages) -->
  <attributes>
    <attr key="{key}" type="{string|number|boolean}">{value}</attr>
    <!-- ... additional attributes ... -->
  </attributes>

  <!-- Operational Status (for CapabilityAdvertisement) -->
  <status operational="{READY|ACTIVE|DEGRADED|OFFLINE|LOADING}"
          readiness="{0.0-1.0}"/>

  <!-- Model Capabilities (for CapabilityAdvertisement) -->
  <capability type="{model_type}"
              model_id="{model_id}"
              model_version="{version}"
              precision="{0.0-1.0}"
              recall="{0.0-1.0}"
              fps="{frames_per_second}"
              status="{status}"/>

  <!-- Resource Metrics (optional) -->
  <resources gpu="{utilization_pct}"
             memory_used_mb="{mb}"
             memory_total_mb="{mb}"/>

  <!-- Handoff Information (for HandoffMessage) -->
  <handoff type="{PREPARE|CONFIRM|RELEASE|FAILED}"
           track_id="{track_id}"
           source="{source_team}"
           target="{target_team}"/>

  <!-- Predicted Position (for handoff) -->
  <predicted lat="{lat}" lon="{lon}"/>

  <!-- POI Description (for handoff) -->
  <poi_description>{description}</poi_description>

  <!-- Classification (for security marking) -->
  <classification level="{UNCLASSIFIED|CUI|SECRET|...}"
                  caveat="{FOUO|NOFORN|...}"/>

  <!-- Formation Summary (for aggregated views) -->
  <formation teams="{count}"
             platforms="{count}"
             cameras="{count}"
             readiness="{0.0-1.0}"/>

  <!-- Tracker Versions (for formation) -->
  <trackers>
    <version>{version_string}</version>
  </trackers>

  <!-- Coverage Sectors (for formation) -->
  <coverage>
    <sector>{sector_id}</sector>
  </coverage>

  <!-- Capability Confidence (for formation) -->
  <capabilities>
    <capability type="{type}" confidence="{0.0-1.0}"/>
  </capabilities>
</_peat_>
```

### Element Specifications

#### `<source>` - Source Attribution

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `platform` | string | Yes | Platform ID that produced this data |
| `model` | string | Yes | AI model ID (for AI-generated data) |
| `model_version` | string | Yes | Semantic version of the model |
| `model_hash` | string | No | Content hash for model verification |

**Example**:
```xml
<source platform="Alpha-2" model="object_tracker" model_version="1.3.0"/>
```

#### `<confidence>` - Track Confidence

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `value` | float | Yes | Confidence score 0.0-1.0 |
| `threshold` | float | No | Minimum acceptable threshold |

**Example**:
```xml
<confidence value="0.89" threshold="0.70"/>
```

#### `<hierarchy>` - Hierarchy Membership

Contains child elements describing the entity's position in Peat hierarchy:

| Element | Attributes | Description |
|---------|------------|-------------|
| `<cell>` | `id`, `role` | Immediate cell membership |
| `<formation>` | `id` | Parent formation |
| `<zone>` | `id` | Operational zone |

**Example**:
```xml
<hierarchy>
  <cell id="Alpha-Team" role="tracker"/>
  <formation id="Formation-1"/>
  <zone id="Zone-A"/>
</hierarchy>
```

#### `<attributes>` - Custom Attributes

Pass-through container for domain-specific metadata from Peat messages.

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `key` | string | Yes | Attribute name |
| `type` | enum | No | `string`, `number`, `boolean` (default: `string`) |

**Example**:
```xml
<attributes>
  <attr key="jacket_color" type="string">blue</attr>
  <attr key="has_backpack" type="boolean">true</attr>
  <attr key="estimated_age" type="string">adult</attr>
  <attr key="confidence_detail" type="number">0.94</attr>
</attributes>
```

#### `<status>` - Operational Status

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `operational` | enum | Yes | `READY`, `ACTIVE`, `DEGRADED`, `OFFLINE`, `LOADING` |
| `readiness` | float | No | Readiness score 0.0-1.0 |

**Status Semantics**:

| Status | Meaning | TAK Display Suggestion |
|--------|---------|----------------------|
| `READY` | Platform ready, not actively processing | Green indicator |
| `ACTIVE` | Platform actively tracking/processing | Blue/pulsing |
| `DEGRADED` | Reduced capability (thermal, low battery, etc.) | Yellow/warning |
| `OFFLINE` | Not responsive | Red/grayed |
| `LOADING` | Model loading or initializing | Gray/spinner |

#### `<capability>` - Model Capability

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `type` | string | Yes | Capability type (e.g., `OBJECT_TRACKING`) |
| `model_id` | string | Yes | Model identifier |
| `model_version` | string | Yes | Semantic version |
| `precision` | float | No | Model precision metric |
| `recall` | float | No | Model recall metric |
| `fps` | float | No | Processing rate |
| `status` | enum | No | Capability-specific status |

#### `<handoff>` - Track Handoff

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `type` | enum | Yes | `PREPARE`, `CONFIRM`, `RELEASE`, `FAILED` |
| `track_id` | string | Yes | Track being handed off |
| `source` | string | Yes | Source team ID |
| `target` | string | Yes | Target team ID |

#### `<classification>` - Security Marking

| Attribute | Type | Required | Description |
|-----------|------|----------|-------------|
| `level` | enum | Yes | Classification level |
| `caveat` | string | No | Handling caveat (FOUO, NOFORN, etc.) |

**Note**: Classification handling must comply with ADR-006 security requirements.

### Complete Examples

#### TrackUpdate Message

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="TRACK-001"
       type="a-f-G-E-S"
       time="2025-11-26T14:10:00Z"
       start="2025-11-26T14:10:00Z"
       stale="2025-11-26T14:10:30Z"
       how="m-g">

  <point lat="33.7749" lon="-84.3958" hae="0.0" ce="2.5" le="9999999.0"/>

  <detail>
    <track course="45.0" speed="1.2"/>
    <remarks>person: blue jacket, has backpack (89% confidence)</remarks>

    <_peat_ version="1.0">
      <source platform="Alpha-2" model="Alpha-3" model_version="1.3.0"/>
      <confidence value="0.89" threshold="0.70"/>
      <hierarchy>
        <cell id="Alpha-Team" role="tracker"/>
        <formation id="Formation-1"/>
      </hierarchy>
      <attributes>
        <attr key="jacket_color">blue</attr>
        <attr key="has_backpack" type="boolean">true</attr>
      </attributes>
    </_peat_>

    <link uid="Alpha-2" type="a-f-G-U-C" relation="o-o"/>
  </detail>
</event>
```

#### CapabilityAdvertisement Message

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="Alpha-3"
       type="a-f-G-U-C"
       time="2025-11-26T14:00:00Z"
       start="2025-11-26T14:00:00Z"
       stale="2025-11-26T14:01:00Z"
       how="m-g">

  <point lat="33.7749" lon="-84.3958" hae="0.0" ce="9999999.0" le="9999999.0"/>

  <detail>
    <contact callsign="Alpha-3"/>
    <remarks>AI Platform: object_tracker v1.3.0 (Active, 91% ready)</remarks>

    <_peat_ version="1.0">
      <status operational="ACTIVE" readiness="0.91"/>
      <capability type="OBJECT_TRACKING"
                  model_id="object_tracker"
                  model_version="1.3.0"
                  precision="0.94"
                  recall="0.89"
                  fps="15.0"
                  status="ACTIVE"/>
      <resources gpu="45" memory_used_mb="2048" memory_total_mb="8192"/>
      <hierarchy>
        <cell id="Alpha-Team" role="ai_platform"/>
      </hierarchy>
    </_peat_>

    <__group name="Alpha-Team" role="AI Platform"/>
  </detail>
</event>
```

#### HandoffMessage

```xml
<?xml version="1.0" encoding="UTF-8"?>
<event version="2.0"
       uid="handoff-12345"
       type="a-x-h-h"
       time="2025-11-26T14:15:00Z"
       start="2025-11-26T14:15:00Z"
       stale="2025-11-26T14:16:00Z"
       how="m-g">

  <point lat="33.7800" lon="-84.3900" hae="0.0" ce="5.0" le="9999999.0"/>

  <detail>
    <remarks>HANDOFF PREPARE: TRACK-001 from Alpha-Team to Bravo-Team</remarks>

    <_peat_ version="1.0">
      <handoff type="PREPARE"
               track_id="TRACK-001"
               source="Alpha-Team"
               target="Bravo-Team"/>
      <poi_description>Person in blue jacket with backpack, heading NE</poi_description>
      <predicted lat="33.7850" lon="-84.3850"/>
      <confidence value="0.85"/>
    </_peat_>

    <link uid="Alpha-Team" type="a-f-G-U-C" relation="h-h" remarks="handoff-source"/>
    <link uid="Bravo-Team" type="a-f-G-U-C" relation="h-h" remarks="handoff-target"/>
    <link uid="TRACK-001" type="a-f-G-E-S" relation="p-p" remarks="track"/>
  </detail>
</event>
```

## Implementation

### Rust Encoding

```rust
use quick_xml::{Writer, events::{Event, BytesStart, BytesText}};

pub struct PeatDetailEncoder;

impl PeatDetailEncoder {
    pub fn encode_track_update(track: &TrackUpdate) -> Result<String, EncodingError> {
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        // Start _peat_ element
        let mut peat = BytesStart::borrowed(b"_peat_", "_peat_".len());
        peat.push_attribute(("version", "1.0"));
        writer.write_event(Event::Start(peat))?;

        // Source attribution
        let mut source = BytesStart::borrowed(b"source", "source".len());
        source.push_attribute(("platform", track.source_platform.as_str()));
        source.push_attribute(("model", track.source_model.as_str()));
        source.push_attribute(("model_version", track.model_version.as_str()));
        writer.write_event(Event::Empty(source))?;

        // Confidence
        let mut confidence = BytesStart::borrowed(b"confidence", "confidence".len());
        confidence.push_attribute(("value", &format!("{:.2}", track.confidence)));
        writer.write_event(Event::Empty(confidence))?;

        // Attributes
        if !track.attributes.is_empty() {
            writer.write_event(Event::Start(BytesStart::borrowed(b"attributes", "attributes".len())))?;
            for (key, value) in &track.attributes {
                let mut attr = BytesStart::borrowed(b"attr", "attr".len());
                attr.push_attribute(("key", key.as_str()));
                writer.write_event(Event::Start(attr))?;
                writer.write_event(Event::Text(BytesText::from_plain_str(&value.to_string())))?;
                writer.write_event(Event::End(BytesEnd::borrowed(b"attr")))?;
            }
            writer.write_event(Event::End(BytesEnd::borrowed(b"attributes")))?;
        }

        // End _peat_ element
        writer.write_event(Event::End(BytesEnd::borrowed(b"_peat_")))?;

        Ok(String::from_utf8(writer.into_inner().into_inner())?)
    }
}
```

### Rust Decoding

```rust
use quick_xml::Reader;

pub struct PeatDetailDecoder;

impl PeatDetailDecoder {
    pub fn decode_peat_extension(xml: &str) -> Result<PeatExtension, DecodingError> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);

        let mut extension = PeatExtension::default();
        let mut buf = Vec::new();

        loop {
            match reader.read_event(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    match e.name() {
                        b"source" => {
                            extension.source = Some(parse_source_attrs(e)?);
                        }
                        b"confidence" => {
                            extension.confidence = Some(parse_confidence_attrs(e)?);
                        }
                        b"status" => {
                            extension.status = Some(parse_status_attrs(e)?);
                        }
                        // ... handle other elements
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(DecodingError::XmlParse(e)),
                _ => {}
            }
            buf.clear();
        }

        Ok(extension)
    }
}
```

### XSD Schema (for validation)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           xmlns:peat="urn:peat:cot:1.0"
           targetNamespace="urn:peat:cot:1.0"
           elementFormDefault="qualified">

  <xs:element name="_peat_">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="source" type="peat:SourceType" minOccurs="0"/>
        <xs:element name="confidence" type="peat:ConfidenceType" minOccurs="0"/>
        <xs:element name="hierarchy" type="peat:HierarchyType" minOccurs="0"/>
        <xs:element name="attributes" type="peat:AttributesType" minOccurs="0"/>
        <xs:element name="status" type="peat:StatusType" minOccurs="0"/>
        <xs:element name="capability" type="peat:CapabilityType" minOccurs="0" maxOccurs="unbounded"/>
        <xs:element name="resources" type="peat:ResourcesType" minOccurs="0"/>
        <xs:element name="handoff" type="peat:HandoffType" minOccurs="0"/>
        <xs:element name="predicted" type="peat:PositionType" minOccurs="0"/>
        <xs:element name="poi_description" type="xs:string" minOccurs="0"/>
        <xs:element name="classification" type="peat:ClassificationType" minOccurs="0"/>
        <xs:element name="formation" type="peat:FormationType" minOccurs="0"/>
      </xs:sequence>
      <xs:attribute name="version" type="xs:string" use="required"/>
    </xs:complexType>
  </xs:element>

  <xs:complexType name="SourceType">
    <xs:attribute name="platform" type="xs:string" use="required"/>
    <xs:attribute name="model" type="xs:string" use="required"/>
    <xs:attribute name="model_version" type="xs:string" use="required"/>
    <xs:attribute name="model_hash" type="xs:string"/>
  </xs:complexType>

  <xs:complexType name="ConfidenceType">
    <xs:attribute name="value" type="peat:UnitInterval" use="required"/>
    <xs:attribute name="threshold" type="peat:UnitInterval"/>
  </xs:complexType>

  <xs:simpleType name="UnitInterval">
    <xs:restriction base="xs:decimal">
      <xs:minInclusive value="0.0"/>
      <xs:maxInclusive value="1.0"/>
    </xs:restriction>
  </xs:simpleType>

  <xs:simpleType name="OperationalStatusEnum">
    <xs:restriction base="xs:string">
      <xs:enumeration value="READY"/>
      <xs:enumeration value="ACTIVE"/>
      <xs:enumeration value="DEGRADED"/>
      <xs:enumeration value="OFFLINE"/>
      <xs:enumeration value="LOADING"/>
    </xs:restriction>
  </xs:simpleType>

  <!-- Additional type definitions... -->

</xs:schema>
```

## Consequences

### Positive

1. **Semantic Preservation**: Rich Peat context survives translation to CoT
2. **TAK Compatibility**: Uses standard CoT extension mechanism
3. **Operator Awareness**: TAK users can see confidence, source, hierarchy
4. **Plugin Support**: ATAK plugins can render Peat-specific UI
5. **Versioned Schema**: Enables forward-compatible evolution
6. **Standardized Format**: Consistent across all Peat message types

### Negative

1. **Message Size**: Adds bytes to every CoT message
2. **Parsing Overhead**: TAK clients must parse extension
3. **Schema Evolution**: Must maintain backward compatibility
4. **Plugin Dependency**: Full benefit requires ATAK plugin development

### Risks and Mitigations

**Risk 1**: Schema changes break existing integrations
- **Mitigation**: Semantic versioning, backward-compatible additions only
- **Mitigation**: Version attribute enables client-side handling

**Risk 2**: TAK servers strip unknown extensions
- **Mitigation**: Use `_` prefix convention (TAK pass-through)
- **Mitigation**: Test with FreeTakServer and official TAK Server

**Risk 3**: Message size impacts bandwidth
- **Mitigation**: Extension is optional for bandwidth-constrained scenarios
- **Mitigation**: Integrate with ADR-019 QoS for adaptive detail level

## Success Metrics

1. **Completeness**: All Peat message types have defined mappings
2. **Round-trip**: Peat → CoT → Peat preserves semantic meaning
3. **Compatibility**: Works with TAK Server, FreeTakServer, ATAK
4. **Performance**: Extension parsing < 1ms
5. **Documentation**: XSD schema published and validated

## References

1. [CoT XML Schema](http://cot.mitre.org)
2. [TAK.gov Developer Resources](https://tak.gov)
3. [quick-xml crate](https://docs.rs/quick-xml/latest/quick_xml/)
4. [M1 POC TAK Integration Requirements](TAK_INTEGRATION_REQUIREMENTS.md)
5. [CoT Schema Mapping](COT_SCHEMA_MAPPING.md)

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-26 | Created ADR-028 | M1 POC feedback - need standardized Peat extension |
| 2025-11-26 | Selected `_peat_` element name | TAK `_` prefix convention for extensions |
| 2025-11-26 | Included all Peat message types | Comprehensive coverage from COT_SCHEMA_MAPPING.md |
