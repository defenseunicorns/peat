//! PEAT Protocol Ontology
//!
//! This module defines the semantic vocabulary and relationships for the PEAT Protocol.
//! It provides:
//! - Domain concepts and their relationships
//! - Semantic validation rules
//! - Ontology-based reasoning utilities
//!
//! The ontology is based on:
//! - Military domain knowledge (doctrine, tactics)
//! - Autonomous systems concepts
//! - CRDT theory and distributed systems

use std::collections::HashMap;

/// Ontology concept categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConceptCategory {
    /// Physical entities (nodes, platforms)
    Entity,
    /// Organizational structures (cells, platoons)
    Organization,
    /// Capabilities and skills
    Capability,
    /// Processes and activities
    Process,
    /// Roles and responsibilities
    Role,
}

/// Ontology concept definition
#[derive(Debug, Clone)]
pub struct Concept {
    /// Concept identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Category
    pub category: ConceptCategory,
    /// Description
    pub description: String,
    /// Parent concepts (is-a relationships)
    pub parents: Vec<String>,
    /// Related concepts
    pub related: Vec<String>,
    /// Properties (key-value metadata)
    pub properties: HashMap<String, String>,
}

impl Concept {
    /// Create a new concept
    pub fn new(id: impl Into<String>, name: impl Into<String>, category: ConceptCategory) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
            description: String::new(),
            parents: Vec::new(),
            related: Vec::new(),
            properties: HashMap::new(),
        }
    }

    /// Add a parent concept (is-a relationship)
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parents.push(parent_id.into());
        self
    }

    /// Add a related concept
    pub fn with_related(mut self, related_id: impl Into<String>) -> Self {
        self.related.push(related_id.into());
        self
    }

    /// Add a property
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

/// PEAT Protocol ontology
pub struct Ontology {
    concepts: HashMap<String, Concept>,
}

impl Ontology {
    /// Create a new ontology
    pub fn new() -> Self {
        Self {
            concepts: HashMap::new(),
        }
    }

    /// Add a concept to the ontology
    pub fn add_concept(&mut self, concept: Concept) {
        self.concepts.insert(concept.id.clone(), concept);
    }

    /// Get a concept by ID
    pub fn get_concept(&self, id: &str) -> Option<&Concept> {
        self.concepts.get(id)
    }

    /// Check if a concept is a subtype of another (transitive is-a check)
    pub fn is_subtype_of(&self, concept_id: &str, parent_id: &str) -> bool {
        if concept_id == parent_id {
            return true;
        }

        if let Some(concept) = self.get_concept(concept_id) {
            for parent in &concept.parents {
                if self.is_subtype_of(parent, parent_id) {
                    return true;
                }
            }
        }

        false
    }

    /// Get all concepts in a category
    pub fn concepts_by_category(&self, category: ConceptCategory) -> Vec<&Concept> {
        self.concepts
            .values()
            .filter(|c| c.category == category)
            .collect()
    }
}

impl Default for Ontology {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the PEAT Protocol domain ontology
pub fn build_cap_ontology() -> Ontology {
    let mut ont = Ontology::new();

    // Entity concepts
    ont.add_concept(
        Concept::new("node", "Node", ConceptCategory::Entity)
            .with_description("A node in the PEAT Protocol network (platform or system)"),
    );

    ont.add_concept(
        Concept::new("platform", "Platform", ConceptCategory::Entity)
            .with_parent("node")
            .with_description("A physical platform (UAV, UGV, soldier system, etc.)"),
    );

    ont.add_concept(
        Concept::new("uav", "Unmanned Aerial Vehicle", ConceptCategory::Entity)
            .with_parent("platform")
            .with_property("domain", "air"),
    );

    ont.add_concept(
        Concept::new("ugv", "Unmanned Ground Vehicle", ConceptCategory::Entity)
            .with_parent("platform")
            .with_property("domain", "ground"),
    );

    ont.add_concept(
        Concept::new("soldier_system", "Soldier System", ConceptCategory::Entity)
            .with_parent("platform")
            .with_property("domain", "ground")
            .with_property("human_operated", "true"),
    );

    // Organization concepts
    ont.add_concept(
        Concept::new("cell", "Cell", ConceptCategory::Organization)
            .with_description("A tactical cell (squad-level formation)")
            .with_property("min_size", "2")
            .with_property("max_size", "8"),
    );

    ont.add_concept(
        Concept::new("platoon", "Platoon", ConceptCategory::Organization)
            .with_description("A platoon-level formation of multiple cells")
            .with_property("min_size", "3")
            .with_property("max_size", "4"),
    );

    ont.add_concept(
        Concept::new("company", "Company", ConceptCategory::Organization)
            .with_description("A company-level formation of multiple platoons"),
    );

    // Capability concepts
    ont.add_concept(
        Concept::new("capability", "Capability", ConceptCategory::Capability)
            .with_description("A capability that a node or cell possesses"),
    );

    ont.add_concept(
        Concept::new("sensor", "Sensor", ConceptCategory::Capability)
            .with_parent("capability")
            .with_description("Sensing capability (cameras, radar, etc.)"),
    );

    ont.add_concept(
        Concept::new("compute", "Compute", ConceptCategory::Capability)
            .with_parent("capability")
            .with_description("Computing capability (processing, AI/ML)"),
    );

    ont.add_concept(
        Concept::new(
            "communication",
            "Communication",
            ConceptCategory::Capability,
        )
        .with_parent("capability")
        .with_description("Communication capability (radio, network)"),
    );

    ont.add_concept(
        Concept::new("mobility", "Mobility", ConceptCategory::Capability)
            .with_parent("capability")
            .with_description("Mobility capability (movement, navigation)"),
    );

    ont.add_concept(
        Concept::new("payload", "Payload", ConceptCategory::Capability)
            .with_parent("capability")
            .with_description("Payload capability (weapons, cargo)"),
    );

    ont.add_concept(
        Concept::new(
            "emergent",
            "Emergent Capability",
            ConceptCategory::Capability,
        )
        .with_parent("capability")
        .with_description("Emergent capability from composition"),
    );

    // Process concepts
    ont.add_concept(
        Concept::new("discovery", "Discovery", ConceptCategory::Process)
            .with_description("Node discovery phase (beacon broadcasting)"),
    );

    ont.add_concept(
        Concept::new("cell_formation", "Cell Formation", ConceptCategory::Process)
            .with_description("Cell formation phase (capability composition)"),
    );

    ont.add_concept(
        Concept::new(
            "hierarchy",
            "Hierarchical Operations",
            ConceptCategory::Process,
        )
        .with_description("Hierarchical operations phase (platoon/company)"),
    );

    ont.add_concept(
        Concept::new("composition", "Composition", ConceptCategory::Process)
            .with_description("Capability composition process"),
    );

    // Role concepts
    ont.add_concept(
        Concept::new("leader", "Leader", ConceptCategory::Role)
            .with_description("Cell leader role"),
    );

    ont.add_concept(
        Concept::new("member", "Member", ConceptCategory::Role)
            .with_description("Cell member role"),
    );

    ont.add_concept(
        Concept::new("operator", "Operator", ConceptCategory::Role)
            .with_description("Human operator of a platform"),
    );

    ont
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology_creation() {
        let ont = build_cap_ontology();

        // Check entity concepts exist
        assert!(ont.get_concept("node").is_some());
        assert!(ont.get_concept("platform").is_some());
        assert!(ont.get_concept("uav").is_some());

        // Check organization concepts exist
        assert!(ont.get_concept("cell").is_some());
        assert!(ont.get_concept("platoon").is_some());
    }

    #[test]
    fn test_ontology_is_subtype() {
        let ont = build_cap_ontology();

        // UAV is a platform
        assert!(ont.is_subtype_of("uav", "platform"));

        // Platform is a node
        assert!(ont.is_subtype_of("platform", "node"));

        // UAV is a node (transitive)
        assert!(ont.is_subtype_of("uav", "node"));

        // Cell is not a platform
        assert!(!ont.is_subtype_of("cell", "platform"));

        // Reflexive check
        assert!(ont.is_subtype_of("node", "node"));
    }

    #[test]
    fn test_ontology_concepts_by_category() {
        let ont = build_cap_ontology();

        let entities = ont.concepts_by_category(ConceptCategory::Entity);
        assert!(entities.len() >= 4); // node, platform, uav, ugv, soldier_system

        let capabilities = ont.concepts_by_category(ConceptCategory::Capability);
        assert!(capabilities.len() >= 7); // capability + 6 types
    }
}
