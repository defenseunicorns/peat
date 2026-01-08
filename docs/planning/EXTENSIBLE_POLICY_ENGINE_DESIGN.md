# Extensible Policy Engine Design

## Problem

Current policy engine is tightly coupled to `HierarchicalCommand` and cannot be reused for other domain models (Node, Cell, Capability, etc.).

## Proposed Solution: Generic Policy Engine with Traits

### 1. Core Trait: `Conflictable`

Define a trait that any type can implement to participate in conflict resolution:

```rust
/// Trait for types that can participate in conflict resolution
pub trait Conflictable: Clone + Send + Sync {
    /// Unique identifier for this item
    fn id(&self) -> String;

    /// Resource keys this item affects (for conflict detection)
    ///
    /// Examples:
    /// - Commands: target_ids
    /// - Nodes: geographic area, capability types
    /// - Cells: squad_id, resource allocations
    fn conflict_keys(&self) -> Vec<String>;

    /// Timestamp for recency-based resolution
    fn timestamp(&self) -> Option<u64>;

    /// Custom attributes for policy resolution
    ///
    /// Allows policies to access type-specific data without knowing the type
    fn attributes(&self) -> HashMap<String, AttributeValue>;
}

/// Generic attribute values for policy resolution
#[derive(Debug, Clone)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Timestamp(u64),
}
```

### 2. Generic Policy Engine

```rust
/// Generic conflict resolver for any `Conflictable` type
pub struct GenericConflictResolver<T: Conflictable> {
    /// Active items indexed by conflict key
    active_items: Arc<RwLock<HashMap<String, Vec<T>>>>,
    _phantom: PhantomData<T>,
}

impl<T: Conflictable> GenericConflictResolver<T> {
    pub fn new() -> Self {
        Self {
            active_items: Arc::new(RwLock::new(HashMap::new())),
            _phantom: PhantomData,
        }
    }

    pub async fn check_conflict(&self, item: &T) -> ConflictResult<T> {
        let keys = item.conflict_keys();
        let items = self.active_items.read().await;

        let mut conflicting = Vec::new();
        for key in keys {
            if let Some(existing) = items.get(&key) {
                conflicting.extend(existing.clone());
            }
        }

        if conflicting.is_empty() {
            ConflictResult::NoConflict
        } else {
            ConflictResult::Conflict(conflicting)
        }
    }

    pub fn resolve(
        &self,
        items: Vec<T>,
        policy: &dyn ResolutionPolicy<T>,
    ) -> Result<T> {
        policy.resolve(items)
    }
}
```

### 3. Policy Trait for Custom Resolution Logic

```rust
/// Trait for conflict resolution policies
pub trait ResolutionPolicy<T: Conflictable>: Send + Sync {
    /// Resolve conflict between multiple items
    fn resolve(&self, items: Vec<T>) -> Result<T>;

    /// Policy name for logging/debugging
    fn name(&self) -> &str;
}

/// Generic "last write wins" policy
pub struct LastWriteWinsPolicy;

impl<T: Conflictable> ResolutionPolicy<T> for LastWriteWinsPolicy {
    fn resolve(&self, mut items: Vec<T>) -> Result<T> {
        items.sort_by(|a, b| {
            let a_time = a.timestamp().unwrap_or(0);
            let b_time = b.timestamp().unwrap_or(0);
            b_time.cmp(&a_time) // Most recent first
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        "LAST_WRITE_WINS"
    }
}

/// Generic priority-based policy
pub struct HighestAttributeWinsPolicy {
    attribute_name: String,
}

impl<T: Conflictable> ResolutionPolicy<T> for HighestAttributeWinsPolicy {
    fn resolve(&self, mut items: Vec<T>) -> Result<T> {
        items.sort_by(|a, b| {
            let a_val = a.attributes().get(&self.attribute_name);
            let b_val = b.attributes().get(&self.attribute_name);

            match (a_val, b_val) {
                (Some(AttributeValue::Int(a)), Some(AttributeValue::Int(b))) => b.cmp(a),
                _ => std::cmp::Ordering::Equal,
            }
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        &self.attribute_name
    }
}
```

### 4. Implementations for Domain Models

#### Commands

```rust
impl Conflictable for HierarchicalCommand {
    fn id(&self) -> String {
        self.command_id.clone()
    }

    fn conflict_keys(&self) -> Vec<String> {
        self.target
            .as_ref()
            .map(|t| t.target_ids.clone())
            .unwrap_or_default()
    }

    fn timestamp(&self) -> Option<u64> {
        self.issued_at.as_ref().map(|t| t.seconds)
    }

    fn attributes(&self) -> HashMap<String, AttributeValue> {
        let mut attrs = HashMap::new();
        attrs.insert("priority".to_string(), AttributeValue::Int(self.priority as i64));
        attrs.insert("originator_id".to_string(), AttributeValue::String(self.originator_id.clone()));

        // Derive authority level from originator
        let authority = if self.originator_id.starts_with("zone-") {
            3
        } else if self.originator_id.starts_with("squad-") {
            2
        } else {
            1
        };
        attrs.insert("authority_level".to_string(), AttributeValue::Int(authority));

        attrs
    }
}

// Now CommandCoordinator uses the generic resolver
pub struct CommandCoordinator {
    conflict_resolver: Arc<GenericConflictResolver<HierarchicalCommand>>,
    // ... other fields
}
```

#### Nodes (Geographic Conflict)

```rust
impl Conflictable for NodeConfig {
    fn id(&self) -> String {
        self.node_id.clone()
    }

    fn conflict_keys(&self) -> Vec<String> {
        // Nodes conflict if operating in same geographic area
        self.position
            .as_ref()
            .map(|pos| vec![format!("geo:{}:{}", pos.latitude, pos.longitude)])
            .unwrap_or_default()
    }

    fn timestamp(&self) -> Option<u64> {
        self.last_updated.as_ref().map(|t| t.seconds)
    }

    fn attributes(&self) -> HashMap<String, AttributeValue> {
        let mut attrs = HashMap::new();
        attrs.insert("platform_type".to_string(), AttributeValue::String(self.platform_type.clone()));
        attrs.insert("autonomy_level".to_string(), AttributeValue::Int(self.autonomy_level as i64));
        attrs
    }
}
```

#### Capabilities (Resource Conflict)

```rust
impl Conflictable for Capability {
    fn id(&self) -> String {
        format!("{}:{}", self.capability_type, self.id)
    }

    fn conflict_keys(&self) -> Vec<String> {
        // Capabilities conflict if they require the same limited resources
        // e.g., "sensor:camera", "actuator:gripper"
        vec![format!("resource:{}", self.capability_type)]
    }

    fn timestamp(&self) -> Option<u64> {
        None // Capabilities don't have timestamps
    }

    fn attributes(&self) -> HashMap<String, AttributeValue> {
        let mut attrs = HashMap::new();
        attrs.insert("confidence".to_string(), AttributeValue::Float(self.confidence));
        attrs.insert("capability_type".to_string(), AttributeValue::String(self.capability_type.clone()));
        attrs
    }
}
```

### 5. Usage Examples

#### Commands (as before)

```rust
let resolver = GenericConflictResolver::<HierarchicalCommand>::new();

let cmd1 = HierarchicalCommand { /* ... */ };
let cmd2 = HierarchicalCommand { /* ... */ };

// Check conflict
let result = resolver.check_conflict(&cmd2).await;

if let ConflictResult::Conflict(existing) = result {
    // Resolve using priority policy
    let policy = HighestAttributeWinsPolicy {
        attribute_name: "priority".to_string(),
    };

    let winner = resolver.resolve(
        vec![existing[0].clone(), cmd2.clone()],
        &policy,
    )?;
}
```

#### Nodes (new use case)

```rust
let resolver = GenericConflictResolver::<NodeConfig>::new();

let node1 = NodeConfig {
    node_id: "drone-1".to_string(),
    position: Some(Position { latitude: 37.7749, longitude: -122.4194 }),
    autonomy_level: 3,
    // ...
};

let node2 = NodeConfig {
    node_id: "drone-2".to_string(),
    position: Some(Position { latitude: 37.7749, longitude: -122.4194 }), // Same location!
    autonomy_level: 5,
    // ...
};

resolver.register_item(&node1).await?;

let result = resolver.check_conflict(&node2).await;

if let ConflictResult::Conflict(_) = result {
    // Resolve: higher autonomy level wins
    let policy = HighestAttributeWinsPolicy {
        attribute_name: "autonomy_level".to_string(),
    };

    let winner = resolver.resolve(vec![node1, node2], &policy)?;
    println!("Node {} wins geographic conflict", winner.id());
}
```

#### Custom Policies

```rust
/// Policy that prefers ISR (surveillance) capabilities over kinetic
pub struct MissionPriorityPolicy;

impl ResolutionPolicy<Capability> for MissionPriorityPolicy {
    fn resolve(&self, mut items: Vec<Capability>) -> Result<Capability> {
        items.sort_by(|a, b| {
            let a_priority = if a.capability_type.contains("ISR") { 10 } else { 1 };
            let b_priority = if b.capability_type.contains("ISR") { 10 } else { 1 };
            b_priority.cmp(&a_priority)
        });

        Ok(items.into_iter().next().unwrap())
    }

    fn name(&self) -> &str {
        "MISSION_PRIORITY_ISR_FIRST"
    }
}
```

## Benefits

1. **Type Agnostic**: Works with Commands, Nodes, Cells, Capabilities, or any custom type
2. **Policy Composability**: Mix and match policies, create custom policies easily
3. **Attribute-Based**: Access to arbitrary attributes without knowing concrete types
4. **Zero Runtime Cost**: Generic specialization means no vtable overhead
5. **Backward Compatible**: Current command-specific resolver can coexist

## Migration Path

### Phase 1: Add Generic Traits (Non-Breaking)
- Add `Conflictable` trait
- Add `GenericConflictResolver<T>`
- Add `ResolutionPolicy<T>` trait
- Keep existing `ConflictResolver` for backward compatibility

### Phase 2: Implement for Domain Models
- Implement `Conflictable` for `HierarchicalCommand`
- Implement `Conflictable` for `NodeConfig`
- Implement `Conflictable` for `Capability`
- Implement `Conflictable` for `Cell`

### Phase 3: Migrate CommandCoordinator
- Replace `Arc<ConflictResolver>` with `Arc<GenericConflictResolver<HierarchicalCommand>>`
- Adapter layer if needed for policy enum → trait object

### Phase 4: Extend to Other Coordinators
- Create `NodeConflictCoordinator` using `GenericConflictResolver<NodeConfig>`
- Create `CellFormationCoordinator` using `GenericConflictResolver<Cell>`

## Future Enhancements

1. **Multi-Key Conflicts**: Items can conflict on multiple dimensions
   ```rust
   fn conflict_keys(&self) -> HashMap<String, Vec<String>> {
       hashmap! {
           "target" => vec!["node-1"],
           "resource" => vec!["cpu:80%", "memory:512MB"],
           "time" => vec!["2025-11-09T10:00:00Z"],
       }
   }
   ```

2. **Conflict Severity**: Some conflicts are hard (must resolve), others are soft (warn only)
   ```rust
   fn conflict_severity(&self, other: &Self) -> ConflictSeverity {
       ConflictSeverity::MustResolve | ConflictSeverity::Warning
   }
   ```

3. **Policy Chaining**: Combine multiple policies
   ```rust
   let policy = PolicyChain::new()
       .first(HighestAuthorityWinsPolicy)
       .then_if_tied(HighestPriorityWinsPolicy)
       .then_if_tied(LastWriteWinsPolicy);
   ```

4. **Async Policies**: Policies that query external systems
   ```rust
   #[async_trait]
   pub trait AsyncResolutionPolicy<T: Conflictable> {
       async fn resolve(&self, items: Vec<T>) -> Result<T>;
   }
   ```

## Summary

The current implementation is **command-specific and not extensible** to other models. The proposed **generic trait-based design** allows:

- ✅ Same policy engine for Commands, Nodes, Cells, Capabilities
- ✅ Custom conflict detection logic per type
- ✅ Pluggable policies via trait objects
- ✅ Attribute-based resolution without knowing concrete types
- ✅ Backward compatible migration path
- ✅ Zero runtime overhead (generics, not trait objects)

Would you like me to implement this extensible version?
