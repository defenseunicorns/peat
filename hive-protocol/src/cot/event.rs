//! CoT Event structure and XML encoding
//!
//! Implements the Cursor-on-Target XML schema for TAK integration.

use chrono::{DateTime, Duration, Utc};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Reader;
use quick_xml::Writer;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use super::hive_extension::HiveExtension;
use super::type_mapper::{CotRelation, CotType};

/// CoT Event - the root element of a Cursor-on-Target message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CotEvent {
    /// CoT version (default "2.0")
    pub version: String,
    /// Unique identifier for this event
    pub uid: String,
    /// CoT type code (MIL-STD-2525 derived)
    pub cot_type: CotType,
    /// Event generation time
    pub time: DateTime<Utc>,
    /// Event validity start time
    pub start: DateTime<Utc>,
    /// Event stale time (when it expires)
    pub stale: DateTime<Utc>,
    /// How the event was generated (m-g = machine-generated)
    pub how: String,
    /// Position information
    pub point: CotPoint,
    /// Detail information
    pub detail: CotDetail,
}

impl CotEvent {
    /// Create a new builder for CotEvent
    pub fn builder() -> CotEventBuilder {
        CotEventBuilder::new()
    }

    /// Encode the event as XML string
    pub fn to_xml(&self) -> Result<String, CotError> {
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        // XML declaration
        writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        // Event element
        let time_str = self.format_time(&self.time);
        let start_str = self.format_time(&self.start);
        let stale_str = self.format_time(&self.stale);

        let mut event_elem = BytesStart::new("event");
        event_elem.push_attribute(("version", self.version.as_str()));
        event_elem.push_attribute(("uid", self.uid.as_str()));
        event_elem.push_attribute(("type", self.cot_type.as_str()));
        event_elem.push_attribute(("time", time_str.as_str()));
        event_elem.push_attribute(("start", start_str.as_str()));
        event_elem.push_attribute(("stale", stale_str.as_str()));
        event_elem.push_attribute(("how", self.how.as_str()));

        writer
            .write_event(Event::Start(event_elem))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        // Point element
        self.write_point(&mut writer)?;

        // Detail element
        self.write_detail(&mut writer)?;

        // Close event
        writer
            .write_event(Event::End(BytesEnd::new("event")))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        let result = writer.into_inner().into_inner();
        String::from_utf8(result).map_err(|e| CotError::Encoding(e.to_string()))
    }

    /// Parse a CoT event from XML string (Issue #318)
    ///
    /// Supports parsing mission task events and other CoT messages from TAK Server.
    pub fn from_xml(xml: &str) -> Result<Self, CotError> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);

        let mut uid = None;
        let mut cot_type = None;
        let mut time = None;
        let mut start = None;
        let mut stale = None;
        let mut how = String::from("m-g");
        let mut point = None;
        let mut detail = CotDetail::default();

        let mut buf = Vec::new();
        let mut in_detail = false;
        let mut in_remarks = false;
        let mut remarks_text = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name();
                    match name.as_ref() {
                        b"event" => {
                            // Parse event attributes
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"uid" => {
                                        uid =
                                            Some(String::from_utf8_lossy(&attr.value).into_owned());
                                    }
                                    b"type" => {
                                        cot_type = Some(CotType::new(&String::from_utf8_lossy(
                                            &attr.value,
                                        )));
                                    }
                                    b"time" => {
                                        time = Self::parse_time(&attr.value);
                                    }
                                    b"start" => {
                                        start = Self::parse_time(&attr.value);
                                    }
                                    b"stale" => {
                                        stale = Self::parse_time(&attr.value);
                                    }
                                    b"how" => {
                                        how = String::from_utf8_lossy(&attr.value).into_owned();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        b"point" => {
                            let mut lat = 0.0;
                            let mut lon = 0.0;
                            let mut hae = 0.0;
                            let mut ce = 9999999.0;
                            let mut le = 9999999.0;

                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"lat" => {
                                        lat = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(0.0);
                                    }
                                    b"lon" => {
                                        lon = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(0.0);
                                    }
                                    b"hae" => {
                                        hae = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(0.0);
                                    }
                                    b"ce" => {
                                        ce = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(9999999.0);
                                    }
                                    b"le" => {
                                        le = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(9999999.0);
                                    }
                                    _ => {}
                                }
                            }
                            point = Some(CotPoint::with_full(lat, lon, hae, ce, le));
                        }
                        b"detail" => {
                            in_detail = true;
                        }
                        b"track" if in_detail => {
                            let mut course = 0.0;
                            let mut speed = 0.0;
                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"course" => {
                                        course = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(0.0);
                                    }
                                    b"speed" => {
                                        speed = String::from_utf8_lossy(&attr.value)
                                            .parse()
                                            .unwrap_or(0.0);
                                    }
                                    _ => {}
                                }
                            }
                            detail.track = Some(CotTrack { course, speed });
                        }
                        b"contact" if in_detail => {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"callsign" {
                                    detail.contact_callsign =
                                        Some(String::from_utf8_lossy(&attr.value).into_owned());
                                }
                            }
                        }
                        b"remarks" if in_detail => {
                            in_remarks = true;
                            remarks_text.clear();
                        }
                        b"link" if in_detail => {
                            let mut link_uid = String::new();
                            let mut link_type = String::new();
                            let mut relation = String::new();
                            let mut link_remarks = None;

                            for attr in e.attributes().flatten() {
                                match attr.key.as_ref() {
                                    b"uid" => {
                                        link_uid =
                                            String::from_utf8_lossy(&attr.value).into_owned();
                                    }
                                    b"type" => {
                                        link_type =
                                            String::from_utf8_lossy(&attr.value).into_owned();
                                    }
                                    b"relation" => {
                                        relation =
                                            String::from_utf8_lossy(&attr.value).into_owned();
                                    }
                                    b"remarks" => {
                                        link_remarks =
                                            Some(String::from_utf8_lossy(&attr.value).into_owned());
                                    }
                                    _ => {}
                                }
                            }
                            if !link_uid.is_empty() {
                                detail.links.push(CotLink {
                                    uid: link_uid,
                                    cot_type: link_type,
                                    relation,
                                    remarks: link_remarks,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_remarks {
                        remarks_text.push_str(&e.unescape().unwrap_or_default());
                    }
                }
                Ok(Event::End(ref e)) => match e.name().as_ref() {
                    b"detail" => in_detail = false,
                    b"remarks" => {
                        in_remarks = false;
                        if !remarks_text.is_empty() {
                            detail.remarks = Some(remarks_text.clone());
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(CotError::XmlRead(format!(
                        "XML parse error at position {}: {:?}",
                        reader.buffer_position(),
                        e
                    )));
                }
                _ => {}
            }
            buf.clear();
        }

        let uid = uid.ok_or(CotError::MissingField("uid"))?;
        let cot_type = cot_type.ok_or(CotError::MissingField("type"))?;
        let point = point.ok_or(CotError::MissingField("point"))?;
        let time = time.unwrap_or_else(Utc::now);
        let start = start.unwrap_or(time);
        let stale = stale.unwrap_or(time + Duration::minutes(5));

        Ok(CotEvent {
            version: "2.0".to_string(),
            uid,
            cot_type,
            time,
            start,
            stale,
            how,
            point,
            detail,
        })
    }

    /// Parse ISO 8601 timestamp from bytes
    fn parse_time(value: &[u8]) -> Option<DateTime<Utc>> {
        let s = String::from_utf8_lossy(value);
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| {
                // Try alternative format without timezone
                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.fZ")
                    .ok()
                    .map(|ndt| ndt.and_utc())
            })
            .or_else(|| {
                // Try another common TAK format
                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%SZ")
                    .ok()
                    .map(|ndt| ndt.and_utc())
            })
    }

    fn format_time(&self, time: &DateTime<Utc>) -> String {
        time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }

    fn write_point(&self, writer: &mut Writer<Cursor<Vec<u8>>>) -> Result<(), CotError> {
        let lat_str = self.point.lat.to_string();
        let lon_str = self.point.lon.to_string();
        let hae_str = self.point.hae.to_string();
        let ce_str = self.point.ce.to_string();
        let le_str = self.point.le.to_string();

        let mut point_elem = BytesStart::new("point");
        point_elem.push_attribute(("lat", lat_str.as_str()));
        point_elem.push_attribute(("lon", lon_str.as_str()));
        point_elem.push_attribute(("hae", hae_str.as_str()));
        point_elem.push_attribute(("ce", ce_str.as_str()));
        point_elem.push_attribute(("le", le_str.as_str()));

        writer
            .write_event(Event::Empty(point_elem))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        Ok(())
    }

    fn write_detail(&self, writer: &mut Writer<Cursor<Vec<u8>>>) -> Result<(), CotError> {
        writer
            .write_event(Event::Start(BytesStart::new("detail")))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        // Track element (if present)
        if let Some(ref track) = self.detail.track {
            let course_str = track.course.to_string();
            let speed_str = track.speed.to_string();

            let mut track_elem = BytesStart::new("track");
            track_elem.push_attribute(("course", course_str.as_str()));
            track_elem.push_attribute(("speed", speed_str.as_str()));

            writer
                .write_event(Event::Empty(track_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Contact element (if present)
        if let Some(ref callsign) = self.detail.contact_callsign {
            let mut contact_elem = BytesStart::new("contact");
            contact_elem.push_attribute(("callsign", callsign.as_str()));

            writer
                .write_event(Event::Empty(contact_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Group element (if present)
        if let Some(ref group) = self.detail.group {
            let mut group_elem = BytesStart::new("__group");
            group_elem.push_attribute(("name", group.name.as_str()));
            group_elem.push_attribute(("role", group.role.as_str()));

            writer
                .write_event(Event::Empty(group_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Remarks element (if present)
        if let Some(ref remarks) = self.detail.remarks {
            writer
                .write_event(Event::Start(BytesStart::new("remarks")))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
            writer
                .write_event(Event::Text(BytesText::new(remarks)))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
            writer
                .write_event(Event::End(BytesEnd::new("remarks")))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // HIVE extension (if present)
        if let Some(ref hive) = self.detail.hive_extension {
            hive.write_xml(writer)?;
        }

        // Link elements
        for link in &self.detail.links {
            let mut link_elem = BytesStart::new("link");
            link_elem.push_attribute(("uid", link.uid.as_str()));
            link_elem.push_attribute(("type", link.cot_type.as_str()));
            link_elem.push_attribute(("relation", link.relation.as_str()));
            if let Some(ref remarks) = link.remarks {
                link_elem.push_attribute(("remarks", remarks.as_str()));
            }

            writer
                .write_event(Event::Empty(link_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        // Flow tags (if present)
        if let Some(ref priority) = self.detail.flow_priority {
            let mut flow_elem = BytesStart::new("_flow-tags_");
            flow_elem.push_attribute(("priority", priority.as_str()));

            writer
                .write_event(Event::Empty(flow_elem))
                .map_err(|e| CotError::XmlWrite(e.to_string()))?;
        }

        writer
            .write_event(Event::End(BytesEnd::new("detail")))
            .map_err(|e| CotError::XmlWrite(e.to_string()))?;

        Ok(())
    }
}

/// Point element containing WGS84 coordinates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CotPoint {
    /// Latitude (WGS84)
    pub lat: f64,
    /// Longitude (WGS84)
    pub lon: f64,
    /// Height Above Ellipsoid (meters)
    pub hae: f64,
    /// Circular Error (meters) - horizontal accuracy
    pub ce: f64,
    /// Linear Error (meters) - vertical accuracy
    pub le: f64,
}

impl CotPoint {
    /// Create a new point with default accuracy values
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            lat,
            lon,
            hae: 0.0,
            ce: 9999999.0, // Unknown accuracy
            le: 9999999.0, // Unknown accuracy
        }
    }

    /// Create a point with full position data
    pub fn with_full(lat: f64, lon: f64, hae: f64, ce: f64, le: f64) -> Self {
        Self {
            lat,
            lon,
            hae,
            ce,
            le,
        }
    }
}

/// Detail element containing supplementary information
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CotDetail {
    /// Track information (course/speed)
    pub track: Option<CotTrack>,
    /// Contact callsign
    pub contact_callsign: Option<String>,
    /// Group membership
    pub group: Option<CotGroup>,
    /// Remarks/description
    pub remarks: Option<String>,
    /// HIVE custom extension
    pub hive_extension: Option<HiveExtension>,
    /// Related entity links
    pub links: Vec<CotLink>,
    /// Flow priority for QoS
    pub flow_priority: Option<String>,
}

/// Track element with course and speed
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CotTrack {
    /// Course/bearing in degrees
    pub course: f64,
    /// Speed in meters per second
    pub speed: f64,
}

/// Group membership element
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CotGroup {
    /// Group name
    pub name: String,
    /// Role within group
    pub role: String,
}

/// Link to related entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CotLink {
    /// UID of linked entity
    pub uid: String,
    /// CoT type of linked entity
    pub cot_type: String,
    /// Relationship type
    pub relation: String,
    /// Optional remarks
    pub remarks: Option<String>,
}

impl CotLink {
    /// Create a new link
    pub fn new(uid: &str, cot_type: &str, relation: CotRelation) -> Self {
        Self {
            uid: uid.to_string(),
            cot_type: cot_type.to_string(),
            relation: relation.as_str().to_string(),
            remarks: None,
        }
    }

    /// Add remarks
    pub fn with_remarks(mut self, remarks: &str) -> Self {
        self.remarks = Some(remarks.to_string());
        self
    }
}

/// Builder for CotEvent
#[derive(Debug, Default)]
pub struct CotEventBuilder {
    uid: Option<String>,
    cot_type: Option<CotType>,
    time: Option<DateTime<Utc>>,
    stale_duration: Duration,
    how: String,
    point: Option<CotPoint>,
    detail: CotDetail,
}

impl CotEventBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            uid: None,
            cot_type: None,
            time: None,
            stale_duration: Duration::seconds(30),
            how: "m-g".to_string(), // machine-generated
            point: None,
            detail: CotDetail::default(),
        }
    }

    /// Set the UID
    pub fn uid(mut self, uid: &str) -> Self {
        self.uid = Some(uid.to_string());
        self
    }

    /// Set the CoT type
    pub fn cot_type(mut self, cot_type: CotType) -> Self {
        self.cot_type = Some(cot_type);
        self
    }

    /// Set the timestamp
    pub fn time(mut self, time: DateTime<Utc>) -> Self {
        self.time = Some(time);
        self
    }

    /// Set stale duration
    pub fn stale_duration(mut self, duration: Duration) -> Self {
        self.stale_duration = duration;
        self
    }

    /// Set how the event was generated
    pub fn how(mut self, how: &str) -> Self {
        self.how = how.to_string();
        self
    }

    /// Set the point
    pub fn point(mut self, point: CotPoint) -> Self {
        self.point = Some(point);
        self
    }

    /// Set track information
    pub fn track(mut self, course: f64, speed: f64) -> Self {
        self.detail.track = Some(CotTrack { course, speed });
        self
    }

    /// Set contact callsign
    pub fn callsign(mut self, callsign: &str) -> Self {
        self.detail.contact_callsign = Some(callsign.to_string());
        self
    }

    /// Set group membership
    pub fn group(mut self, name: &str, role: &str) -> Self {
        self.detail.group = Some(CotGroup {
            name: name.to_string(),
            role: role.to_string(),
        });
        self
    }

    /// Set remarks
    pub fn remarks(mut self, remarks: &str) -> Self {
        self.detail.remarks = Some(remarks.to_string());
        self
    }

    /// Set HIVE extension
    pub fn hive_extension(mut self, extension: HiveExtension) -> Self {
        self.detail.hive_extension = Some(extension);
        self
    }

    /// Add a link
    pub fn link(mut self, link: CotLink) -> Self {
        self.detail.links.push(link);
        self
    }

    /// Set flow priority (QoS)
    pub fn flow_priority(mut self, priority: &str) -> Self {
        self.detail.flow_priority = Some(priority.to_string());
        self
    }

    /// Build the CotEvent
    pub fn build(self) -> Result<CotEvent, CotError> {
        let uid = self.uid.ok_or(CotError::MissingField("uid"))?;
        let cot_type = self.cot_type.ok_or(CotError::MissingField("cot_type"))?;
        let point = self.point.ok_or(CotError::MissingField("point"))?;
        let time = self.time.unwrap_or_else(Utc::now);

        Ok(CotEvent {
            version: "2.0".to_string(),
            uid,
            cot_type,
            time,
            start: time,
            stale: time + self.stale_duration,
            how: self.how,
            point,
            detail: self.detail,
        })
    }
}

/// Errors during CoT encoding/decoding
#[derive(Debug, Clone, PartialEq)]
pub enum CotError {
    /// Missing required field
    MissingField(&'static str),
    /// XML writing error
    XmlWrite(String),
    /// XML reading/parsing error
    XmlRead(String),
    /// Encoding error
    Encoding(String),
}

impl std::fmt::Display for CotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "Missing required field: {}", field),
            Self::XmlWrite(msg) => write!(f, "XML write error: {}", msg),
            Self::XmlRead(msg) => write!(f, "XML read error: {}", msg),
            Self::Encoding(msg) => write!(f, "Encoding error: {}", msg),
        }
    }
}

impl std::error::Error for CotError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cot_event_builder() {
        let event = CotEvent::builder()
            .uid("TRACK-001")
            .cot_type(CotType::new("a-f-G-E-S"))
            .point(CotPoint::new(33.7749, -84.3958))
            .remarks("Test track")
            .build()
            .unwrap();

        assert_eq!(event.uid, "TRACK-001");
        assert_eq!(event.cot_type.as_str(), "a-f-G-E-S");
        assert_eq!(event.point.lat, 33.7749);
    }

    #[test]
    fn test_cot_event_missing_uid() {
        let result = CotEvent::builder()
            .cot_type(CotType::new("a-f-G"))
            .point(CotPoint::new(0.0, 0.0))
            .build();

        assert!(matches!(result, Err(CotError::MissingField("uid"))));
    }

    #[test]
    fn test_cot_event_to_xml() {
        let event = CotEvent::builder()
            .uid("TEST-001")
            .cot_type(CotType::new("a-f-G-E-S"))
            .point(CotPoint::new(33.7749, -84.3958))
            .remarks("Test event")
            .build()
            .unwrap();

        let xml = event.to_xml().unwrap();

        assert!(xml.contains("<?xml version=\"1.0\""));
        assert!(xml.contains("uid=\"TEST-001\""));
        assert!(xml.contains("type=\"a-f-G-E-S\""));
        assert!(xml.contains("lat=\"33.7749\""));
        assert!(xml.contains("<remarks>Test event</remarks>"));
    }

    #[test]
    fn test_cot_event_with_track() {
        let event = CotEvent::builder()
            .uid("TRACK-001")
            .cot_type(CotType::new("a-f-G-E-S"))
            .point(CotPoint::new(33.7749, -84.3958))
            .track(45.0, 5.0)
            .build()
            .unwrap();

        let xml = event.to_xml().unwrap();
        assert!(xml.contains("course=\"45\""));
        assert!(xml.contains("speed=\"5\""));
    }

    #[test]
    fn test_cot_event_with_links() {
        let event = CotEvent::builder()
            .uid("TRACK-001")
            .cot_type(CotType::new("a-f-G-E-S"))
            .point(CotPoint::new(33.7749, -84.3958))
            .link(
                CotLink::new("Alpha-Team", "a-f-G-U-C", CotRelation::Parent)
                    .with_remarks("parent-cell"),
            )
            .build()
            .unwrap();

        let xml = event.to_xml().unwrap();
        assert!(xml.contains("relation=\"p-p\""));
        assert!(xml.contains("remarks=\"parent-cell\""));
    }

    #[test]
    fn test_cot_point_defaults() {
        let point = CotPoint::new(0.0, 0.0);
        assert_eq!(point.hae, 0.0);
        assert_eq!(point.ce, 9999999.0);
        assert_eq!(point.le, 9999999.0);
    }

    #[test]
    fn test_cot_link_creation() {
        let link = CotLink::new("target-uid", "a-f-G-U-C", CotRelation::Observing);
        assert_eq!(link.relation, "o-o");
    }

    #[test]
    fn test_cot_event_with_group() {
        let event = CotEvent::builder()
            .uid("PLATFORM-001")
            .cot_type(CotType::new("a-f-G-U-C"))
            .point(CotPoint::new(33.7749, -84.3958))
            .group("Alpha-Team", "Team Member")
            .build()
            .unwrap();

        let xml = event.to_xml().unwrap();
        assert!(xml.contains("__group"));
        assert!(xml.contains("name=\"Alpha-Team\""));
    }

    // =========================================================================
    // from_xml() tests (Issue #318)
    // =========================================================================

    #[test]
    fn test_cot_event_from_xml_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event version="2.0" uid="TEST-001" type="a-f-G-E-S"
                   time="2025-12-08T14:10:00Z" start="2025-12-08T14:10:00Z"
                   stale="2025-12-08T14:15:00Z" how="m-g">
                <point lat="33.7749" lon="-84.3958" hae="10.0" ce="5.0" le="3.0"/>
                <detail>
                    <remarks>Test event</remarks>
                </detail>
            </event>"#;

        let event = CotEvent::from_xml(xml).unwrap();

        assert_eq!(event.uid, "TEST-001");
        assert_eq!(event.cot_type.as_str(), "a-f-G-E-S");
        assert_eq!(event.how, "m-g");
        assert_eq!(event.point.lat, 33.7749);
        assert_eq!(event.point.lon, -84.3958);
        assert_eq!(event.point.hae, 10.0);
        assert_eq!(event.point.ce, 5.0);
        assert_eq!(event.point.le, 3.0);
        assert_eq!(event.detail.remarks.as_deref(), Some("Test event"));
    }

    #[test]
    fn test_cot_event_from_xml_roundtrip() {
        // Create an event, serialize to XML, parse back
        let original = CotEvent::builder()
            .uid("ROUNDTRIP-001")
            .cot_type(CotType::new("a-f-G-U-C"))
            .point(CotPoint::with_full(38.8977, -77.0365, 50.0, 10.0, 5.0))
            .remarks("Roundtrip test")
            .track(90.0, 5.5)
            .build()
            .unwrap();

        let xml = original.to_xml().unwrap();
        let parsed = CotEvent::from_xml(&xml).unwrap();

        assert_eq!(parsed.uid, original.uid);
        assert_eq!(parsed.cot_type.as_str(), original.cot_type.as_str());
        assert_eq!(parsed.point.lat, original.point.lat);
        assert_eq!(parsed.point.lon, original.point.lon);
        assert_eq!(parsed.detail.remarks, original.detail.remarks);
        assert!(parsed.detail.track.is_some());
        assert_eq!(parsed.detail.track.as_ref().unwrap().course, 90.0);
        assert_eq!(parsed.detail.track.as_ref().unwrap().speed, 5.5);
    }

    #[test]
    fn test_cot_event_from_xml_mission_task() {
        // Test parsing a mission task CoT event (t-x-m-c-c type)
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event uid="MISSION-001" type="t-x-m-c-c" time="2025-12-08T14:05:00Z"
                   start="2025-12-08T14:05:00Z" stale="2025-12-08T15:05:00Z" how="h-g-i-g-o">
                <point lat="33.7756" lon="-84.3963" hae="0" ce="100" le="100"/>
                <detail>
                    <remarks>Track POI within designated area</remarks>
                </detail>
            </event>"#;

        let event = CotEvent::from_xml(xml).unwrap();

        assert_eq!(event.uid, "MISSION-001");
        assert_eq!(event.cot_type.as_str(), "t-x-m-c-c");
        assert_eq!(event.how, "h-g-i-g-o");
        assert_eq!(event.point.lat, 33.7756);
        assert_eq!(event.point.lon, -84.3963);
    }

    #[test]
    fn test_cot_event_from_xml_with_contact() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event uid="ALPHA-3" type="a-f-G-U-C" time="2025-12-08T14:00:00Z"
                   start="2025-12-08T14:00:00Z" stale="2025-12-08T14:01:00Z" how="m-g">
                <point lat="38.0" lon="-77.0" hae="0" ce="10" le="10"/>
                <detail>
                    <contact callsign="Alpha-3"/>
                </detail>
            </event>"#;

        let event = CotEvent::from_xml(xml).unwrap();

        assert_eq!(event.detail.contact_callsign.as_deref(), Some("Alpha-3"));
    }

    #[test]
    fn test_cot_event_from_xml_missing_uid() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event type="a-f-G" time="2025-12-08T14:00:00Z"
                   start="2025-12-08T14:00:00Z" stale="2025-12-08T14:01:00Z" how="m-g">
                <point lat="0" lon="0" hae="0" ce="10" le="10"/>
                <detail/>
            </event>"#;

        let result = CotEvent::from_xml(xml);
        assert!(matches!(result, Err(CotError::MissingField("uid"))));
    }

    #[test]
    fn test_cot_event_from_xml_missing_point() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <event uid="TEST" type="a-f-G" time="2025-12-08T14:00:00Z"
                   start="2025-12-08T14:00:00Z" stale="2025-12-08T14:01:00Z" how="m-g">
                <detail/>
            </event>"#;

        let result = CotEvent::from_xml(xml);
        assert!(matches!(result, Err(CotError::MissingField("point"))));
    }
}
