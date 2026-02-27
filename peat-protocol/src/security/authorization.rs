//! Role-Based Access Control (RBAC) for Peat Protocol.
//!
//! Implements ADR-006 Layer 4: Role-Based Authorization.
//!
//! # Overview
//!
//! This module provides role-based access control for Peat Protocol operations.
//! Each authenticated entity (device or user) has roles that determine what
//! permissions they have for various operations.
//!
//! # Roles
//!
//! - **Leader**: Squad/cell leader - can command cell, set objectives
//! - **Member**: Squad/cell member - participates in missions
//! - **Observer**: Read-only access to cell state
//! - **Commander**: Mission commander - can direct multiple cells
//! - **Admin**: System configuration access
//!
//! # Example
//!
//! ```ignore
//! use peat_protocol::security::{
//!     Role, Permission, AuthorizationController, AuthorizationContext,
//!     AuthenticatedEntity,
//! };
//!
//! let controller = AuthorizationController::with_default_policy();
//!
//! // Check if a leader can set cell objectives
//! let entity = AuthenticatedEntity::Device(device_identity);
//! let context = AuthorizationContext::for_cell(&cell_id);
//!
//! match controller.check_permission(&entity, Permission::SetCellObjective, &context) {
//!     Ok(()) => println!("Permission granted"),
//!     Err(e) => println!("Permission denied: {}", e),
//! }
//! ```

use super::device_id::DeviceId;
use super::error::SecurityError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::SystemTime;

/// Roles in Peat Protocol.
///
/// Roles determine what permissions an entity has for various operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Squad/cell leader - can command cell, set objectives
    Leader,

    /// Squad/cell member - participates in missions
    Member,

    /// Observer - can view but not command
    Observer,

    /// Mission commander - can direct multiple cells
    Commander,

    /// Administrator - can configure system
    Admin,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::Leader => write!(f, "Leader"),
            Role::Member => write!(f, "Member"),
            Role::Observer => write!(f, "Observer"),
            Role::Commander => write!(f, "Commander"),
            Role::Admin => write!(f, "Admin"),
        }
    }
}

/// Permissions that can be checked for authorization.
///
/// These permissions cover all security-relevant operations in Peat Protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    // Cell operations
    /// Permission to join a cell
    JoinCell,
    /// Permission to leave a cell
    LeaveCell,
    /// Permission to create a new cell
    CreateCell,
    /// Permission to disband an existing cell
    DisbandCell,
    /// Permission to set a cell's leader
    SetCellLeader,
    /// Permission to set a cell's objective
    SetCellObjective,

    // Capability operations
    /// Permission to advertise capabilities
    AdvertiseCapability,
    /// Permission to request capabilities from others
    RequestCapability,

    // Data access
    /// Permission to read cell state
    ReadCellState,
    /// Permission to write/modify cell state
    WriteCellState,
    /// Permission to read node state
    ReadNodeState,
    /// Permission to write/modify node state
    WriteNodeState,
    /// Permission to read telemetry data
    ReadTelemetry,

    // Hierarchical operations
    /// Permission to form a platoon from cells
    FormPlatoon,
    /// Permission to aggregate data to company level
    AggregateToCompany,

    // Human-in-the-loop operations
    /// Permission to approve cell formation
    ApproveFormation,
    /// Permission to veto autonomous commands
    VetoCommand,

    // Administration
    /// Permission to configure network settings
    ConfigureNetwork,
    /// Permission to manage cryptographic keys
    ManageKeys,
    /// Permission to view audit logs
    ViewAuditLog,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Permission::JoinCell => write!(f, "JoinCell"),
            Permission::LeaveCell => write!(f, "LeaveCell"),
            Permission::CreateCell => write!(f, "CreateCell"),
            Permission::DisbandCell => write!(f, "DisbandCell"),
            Permission::SetCellLeader => write!(f, "SetCellLeader"),
            Permission::SetCellObjective => write!(f, "SetCellObjective"),
            Permission::AdvertiseCapability => write!(f, "AdvertiseCapability"),
            Permission::RequestCapability => write!(f, "RequestCapability"),
            Permission::ReadCellState => write!(f, "ReadCellState"),
            Permission::WriteCellState => write!(f, "WriteCellState"),
            Permission::ReadNodeState => write!(f, "ReadNodeState"),
            Permission::WriteNodeState => write!(f, "WriteNodeState"),
            Permission::ReadTelemetry => write!(f, "ReadTelemetry"),
            Permission::FormPlatoon => write!(f, "FormPlatoon"),
            Permission::AggregateToCompany => write!(f, "AggregateToCompany"),
            Permission::ApproveFormation => write!(f, "ApproveFormation"),
            Permission::VetoCommand => write!(f, "VetoCommand"),
            Permission::ConfigureNetwork => write!(f, "ConfigureNetwork"),
            Permission::ManageKeys => write!(f, "ManageKeys"),
            Permission::ViewAuditLog => write!(f, "ViewAuditLog"),
        }
    }
}

/// An authenticated entity that can be authorized for operations.
#[derive(Debug, Clone)]
pub enum AuthenticatedEntity {
    /// A device identified by its DeviceId
    Device(DeviceIdentityInfo),

    /// A human user (placeholder for Phase 3)
    User(UserIdentityInfo),
}

impl AuthenticatedEntity {
    /// Get the entity's identifier as a string.
    pub fn id(&self) -> String {
        match self {
            AuthenticatedEntity::Device(info) => info.device_id.to_hex(),
            AuthenticatedEntity::User(info) => info.username.clone(),
        }
    }

    /// Create a device entity from a DeviceId.
    pub fn from_device_id(device_id: DeviceId) -> Self {
        AuthenticatedEntity::Device(DeviceIdentityInfo {
            device_id,
            device_type: DeviceType::Unknown,
        })
    }
}

/// Device identity information for authorization.
#[derive(Debug, Clone)]
pub struct DeviceIdentityInfo {
    /// The device's unique identifier
    pub device_id: DeviceId,

    /// The type of device
    pub device_type: DeviceType,
}

/// Types of devices in Peat Protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceType {
    /// Unmanned Aerial Vehicle
    Uav,
    /// Unmanned Ground Vehicle
    Ugv,
    /// Command and Control station
    C2Station,
    /// Sensor platform
    Sensor,
    /// Communications relay
    Relay,
    /// Unknown device type
    Unknown,
}

/// User identity information (placeholder for Phase 3).
#[derive(Debug, Clone)]
pub struct UserIdentityInfo {
    /// Username or call sign
    pub username: String,

    /// User's roles
    pub roles: HashSet<Role>,
}

/// Context for authorization decisions.
///
/// Provides situational information needed to determine roles and permissions.
#[derive(Debug, Clone)]
pub struct AuthorizationContext {
    /// Cell being accessed (if applicable)
    pub cell_id: Option<String>,

    /// Node being accessed (if applicable)
    pub node_id: Option<String>,

    /// Hierarchy level of the operation
    pub hierarchy_level: Option<HierarchyLevel>,

    /// Time of access
    pub timestamp: SystemTime,

    /// Additional context for role determination
    pub cell_membership: Option<CellMembershipContext>,
}

impl AuthorizationContext {
    /// Create a context for a cell operation.
    pub fn for_cell(cell_id: &str) -> Self {
        Self {
            cell_id: Some(cell_id.to_string()),
            node_id: None,
            hierarchy_level: Some(HierarchyLevel::Squad),
            timestamp: SystemTime::now(),
            cell_membership: None,
        }
    }

    /// Create a context for a node operation.
    pub fn for_node(node_id: &str) -> Self {
        Self {
            cell_id: None,
            node_id: Some(node_id.to_string()),
            hierarchy_level: None,
            timestamp: SystemTime::now(),
            cell_membership: None,
        }
    }

    /// Create an empty context (for system-wide operations).
    pub fn system() -> Self {
        Self {
            cell_id: None,
            node_id: None,
            hierarchy_level: None,
            timestamp: SystemTime::now(),
            cell_membership: None,
        }
    }

    /// Add cell membership information for role determination.
    pub fn with_membership(mut self, membership: CellMembershipContext) -> Self {
        self.cell_membership = Some(membership);
        self
    }
}

/// Context about cell membership for determining device roles.
#[derive(Debug, Clone)]
pub struct CellMembershipContext {
    /// The leader's device ID (as hex string)
    pub leader_id: Option<String>,

    /// Member device IDs (as hex strings)
    pub member_ids: HashSet<String>,
}

impl CellMembershipContext {
    /// Create a new membership context.
    pub fn new(leader_id: Option<String>, member_ids: HashSet<String>) -> Self {
        Self {
            leader_id,
            member_ids,
        }
    }

    /// Check if a device is the leader.
    pub fn is_leader(&self, device_id: &str) -> bool {
        self.leader_id.as_ref() == Some(&device_id.to_string())
    }

    /// Check if a device is a member.
    pub fn is_member(&self, device_id: &str) -> bool {
        self.member_ids.contains(device_id)
    }
}

/// Organizational hierarchy levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HierarchyLevel {
    /// Individual node
    Node,
    /// Squad level (cell)
    Squad,
    /// Platoon level (aggregated squads)
    Platoon,
    /// Company level (aggregated platoons)
    Company,
    /// Battalion level
    Battalion,
}

/// Authorization policy defining role-to-permission mappings.
#[derive(Debug, Clone)]
pub struct AuthorizationPolicy {
    /// Role to permissions mapping
    role_permissions: HashMap<Role, HashSet<Permission>>,
}

impl AuthorizationPolicy {
    /// Create an empty policy.
    pub fn new() -> Self {
        Self {
            role_permissions: HashMap::new(),
        }
    }

    /// Create the default Peat Protocol authorization policy.
    ///
    /// This implements the policy defined in ADR-006.
    pub fn default_policy() -> Self {
        let mut policy = Self::new();

        // Leader permissions - can command cell
        policy.grant_role(Role::Leader, Permission::SetCellObjective);
        policy.grant_role(Role::Leader, Permission::SetCellLeader);
        policy.grant_role(Role::Leader, Permission::RequestCapability);
        policy.grant_role(Role::Leader, Permission::ReadCellState);
        policy.grant_role(Role::Leader, Permission::WriteCellState);
        policy.grant_role(Role::Leader, Permission::ReadNodeState);
        policy.grant_role(Role::Leader, Permission::WriteNodeState);
        policy.grant_role(Role::Leader, Permission::ReadTelemetry);
        policy.grant_role(Role::Leader, Permission::DisbandCell);

        // Member permissions - participate in missions
        policy.grant_role(Role::Member, Permission::JoinCell);
        policy.grant_role(Role::Member, Permission::LeaveCell);
        policy.grant_role(Role::Member, Permission::AdvertiseCapability);
        policy.grant_role(Role::Member, Permission::ReadCellState);
        policy.grant_role(Role::Member, Permission::WriteNodeState);
        policy.grant_role(Role::Member, Permission::ReadNodeState);
        policy.grant_role(Role::Member, Permission::ReadTelemetry);

        // Observer permissions - read-only
        policy.grant_role(Role::Observer, Permission::ReadCellState);
        policy.grant_role(Role::Observer, Permission::ReadNodeState);
        policy.grant_role(Role::Observer, Permission::ReadTelemetry);

        // Commander permissions - hierarchical operations
        policy.grant_role(Role::Commander, Permission::FormPlatoon);
        policy.grant_role(Role::Commander, Permission::AggregateToCompany);
        policy.grant_role(Role::Commander, Permission::ApproveFormation);
        policy.grant_role(Role::Commander, Permission::VetoCommand);
        policy.grant_role(Role::Commander, Permission::CreateCell);
        policy.grant_role(Role::Commander, Permission::ReadCellState);
        policy.grant_role(Role::Commander, Permission::WriteCellState);
        policy.grant_role(Role::Commander, Permission::ReadNodeState);
        policy.grant_role(Role::Commander, Permission::ReadTelemetry);

        // Admin permissions - system-wide
        policy.grant_role(Role::Admin, Permission::ConfigureNetwork);
        policy.grant_role(Role::Admin, Permission::ManageKeys);
        policy.grant_role(Role::Admin, Permission::ViewAuditLog);
        policy.grant_role(Role::Admin, Permission::CreateCell);
        policy.grant_role(Role::Admin, Permission::DisbandCell);

        policy
    }

    /// Grant a permission to a role.
    pub fn grant_role(&mut self, role: Role, permission: Permission) {
        self.role_permissions
            .entry(role)
            .or_default()
            .insert(permission);
    }

    /// Revoke a permission from a role.
    pub fn revoke_role(&mut self, role: Role, permission: Permission) {
        if let Some(permissions) = self.role_permissions.get_mut(&role) {
            permissions.remove(&permission);
        }
    }

    /// Check if a role has a permission.
    pub fn role_has_permission(&self, role: Role, permission: Permission) -> bool {
        self.role_permissions
            .get(&role)
            .is_some_and(|perms| perms.contains(&permission))
    }

    /// Get all permissions for a role.
    pub fn get_permissions(&self, role: Role) -> HashSet<Permission> {
        self.role_permissions
            .get(&role)
            .cloned()
            .unwrap_or_default()
    }
}

impl Default for AuthorizationPolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

/// Role-based authorization controller.
///
/// Checks permissions for authenticated entities based on their roles
/// and the authorization context.
#[derive(Debug)]
pub struct AuthorizationController {
    /// The authorization policy
    policy: AuthorizationPolicy,
}

impl AuthorizationController {
    /// Create a controller with a custom policy.
    pub fn new(policy: AuthorizationPolicy) -> Self {
        Self { policy }
    }

    /// Create a controller with the default Peat Protocol policy.
    pub fn with_default_policy() -> Self {
        Self::new(AuthorizationPolicy::default_policy())
    }

    /// Check if an entity has a permission in the given context.
    ///
    /// Returns `Ok(())` if the permission is granted, or an error if denied.
    pub fn check_permission(
        &self,
        entity: &AuthenticatedEntity,
        permission: Permission,
        context: &AuthorizationContext,
    ) -> Result<(), SecurityError> {
        // Get roles for the entity in this context
        let roles = self.get_roles(entity, context);

        // Check if any role grants the permission
        let granted = roles
            .iter()
            .any(|role| self.policy.role_has_permission(*role, permission));

        if granted {
            Ok(())
        } else {
            Err(SecurityError::PermissionDenied {
                permission: permission.to_string(),
                entity_id: entity.id(),
                roles: roles.iter().map(|r| r.to_string()).collect(),
            })
        }
    }

    /// Get the roles for an entity in the given context.
    pub fn get_roles(
        &self,
        entity: &AuthenticatedEntity,
        context: &AuthorizationContext,
    ) -> HashSet<Role> {
        let mut roles = HashSet::new();

        match entity {
            AuthenticatedEntity::Device(device_info) => {
                // Devices get roles based on cell membership
                if let Some(membership) = &context.cell_membership {
                    let device_hex = device_info.device_id.to_hex();

                    if membership.is_leader(&device_hex) {
                        roles.insert(Role::Leader);
                    } else if membership.is_member(&device_hex) {
                        roles.insert(Role::Member);
                    } else {
                        // Not a member - observer only
                        roles.insert(Role::Observer);
                    }
                } else {
                    // No cell context - default to observer
                    roles.insert(Role::Observer);
                }
            }
            AuthenticatedEntity::User(user_info) => {
                // Users have explicit roles
                roles = user_info.roles.clone();
            }
        }

        roles
    }

    /// Get the underlying policy.
    pub fn policy(&self) -> &AuthorizationPolicy {
        &self.policy
    }
}

impl Default for AuthorizationController {
    fn default() -> Self {
        Self::with_default_policy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device_id() -> DeviceId {
        let keypair = crate::security::DeviceKeypair::generate();
        keypair.device_id()
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::Leader.to_string(), "Leader");
        assert_eq!(Role::Member.to_string(), "Member");
        assert_eq!(Role::Observer.to_string(), "Observer");
        assert_eq!(Role::Commander.to_string(), "Commander");
        assert_eq!(Role::Admin.to_string(), "Admin");
    }

    #[test]
    fn test_permission_display() {
        assert_eq!(Permission::JoinCell.to_string(), "JoinCell");
        assert_eq!(Permission::SetCellObjective.to_string(), "SetCellObjective");
    }

    #[test]
    fn test_default_policy_leader_permissions() {
        let policy = AuthorizationPolicy::default_policy();

        // Leaders should have these permissions
        assert!(policy.role_has_permission(Role::Leader, Permission::SetCellObjective));
        assert!(policy.role_has_permission(Role::Leader, Permission::SetCellLeader));
        assert!(policy.role_has_permission(Role::Leader, Permission::WriteCellState));

        // Leaders should NOT have admin permissions
        assert!(!policy.role_has_permission(Role::Leader, Permission::ConfigureNetwork));
        assert!(!policy.role_has_permission(Role::Leader, Permission::ManageKeys));
    }

    #[test]
    fn test_default_policy_member_permissions() {
        let policy = AuthorizationPolicy::default_policy();

        // Members should have these permissions
        assert!(policy.role_has_permission(Role::Member, Permission::JoinCell));
        assert!(policy.role_has_permission(Role::Member, Permission::LeaveCell));
        assert!(policy.role_has_permission(Role::Member, Permission::ReadCellState));

        // Members should NOT have leader permissions
        assert!(!policy.role_has_permission(Role::Member, Permission::SetCellObjective));
        assert!(!policy.role_has_permission(Role::Member, Permission::SetCellLeader));
    }

    #[test]
    fn test_default_policy_observer_permissions() {
        let policy = AuthorizationPolicy::default_policy();

        // Observers should only have read permissions
        assert!(policy.role_has_permission(Role::Observer, Permission::ReadCellState));
        assert!(policy.role_has_permission(Role::Observer, Permission::ReadNodeState));
        assert!(policy.role_has_permission(Role::Observer, Permission::ReadTelemetry));

        // Observers should NOT have write permissions
        assert!(!policy.role_has_permission(Role::Observer, Permission::WriteCellState));
        assert!(!policy.role_has_permission(Role::Observer, Permission::WriteNodeState));
    }

    #[test]
    fn test_custom_policy() {
        let mut policy = AuthorizationPolicy::new();

        // Initially no permissions
        assert!(!policy.role_has_permission(Role::Member, Permission::CreateCell));

        // Grant permission
        policy.grant_role(Role::Member, Permission::CreateCell);
        assert!(policy.role_has_permission(Role::Member, Permission::CreateCell));

        // Revoke permission
        policy.revoke_role(Role::Member, Permission::CreateCell);
        assert!(!policy.role_has_permission(Role::Member, Permission::CreateCell));
    }

    #[test]
    fn test_authorization_controller_leader() {
        let controller = AuthorizationController::with_default_policy();
        let device_id = test_device_id();
        let device_hex = device_id.to_hex();

        let entity = AuthenticatedEntity::from_device_id(device_id);

        // Create context where device is the leader
        let membership = CellMembershipContext::new(Some(device_hex), HashSet::new());
        let context = AuthorizationContext::for_cell("test-cell").with_membership(membership);

        // Leader should be able to set objectives
        assert!(controller
            .check_permission(&entity, Permission::SetCellObjective, &context)
            .is_ok());

        // Leader should be able to write cell state
        assert!(controller
            .check_permission(&entity, Permission::WriteCellState, &context)
            .is_ok());

        // Leader should NOT be able to configure network (admin only)
        assert!(controller
            .check_permission(&entity, Permission::ConfigureNetwork, &context)
            .is_err());
    }

    #[test]
    fn test_authorization_controller_member() {
        let controller = AuthorizationController::with_default_policy();
        let device_id = test_device_id();
        let device_hex = device_id.to_hex();

        let entity = AuthenticatedEntity::from_device_id(device_id);

        // Create context where device is a member (not leader)
        let mut members = HashSet::new();
        members.insert(device_hex);
        let membership = CellMembershipContext::new(Some("other-leader".to_string()), members);
        let context = AuthorizationContext::for_cell("test-cell").with_membership(membership);

        // Member should be able to read cell state
        assert!(controller
            .check_permission(&entity, Permission::ReadCellState, &context)
            .is_ok());

        // Member should be able to write node state
        assert!(controller
            .check_permission(&entity, Permission::WriteNodeState, &context)
            .is_ok());

        // Member should NOT be able to set objectives (leader only)
        assert!(controller
            .check_permission(&entity, Permission::SetCellObjective, &context)
            .is_err());
    }

    #[test]
    fn test_authorization_controller_observer() {
        let controller = AuthorizationController::with_default_policy();
        let device_id = test_device_id();

        let entity = AuthenticatedEntity::from_device_id(device_id);

        // Create context where device is neither leader nor member (observer)
        let membership =
            CellMembershipContext::new(Some("some-leader".to_string()), HashSet::new());
        let context = AuthorizationContext::for_cell("test-cell").with_membership(membership);

        // Observer should be able to read
        assert!(controller
            .check_permission(&entity, Permission::ReadCellState, &context)
            .is_ok());

        // Observer should NOT be able to write
        assert!(controller
            .check_permission(&entity, Permission::WriteCellState, &context)
            .is_err());

        // Observer should NOT be able to join (they need member role first)
        assert!(controller
            .check_permission(&entity, Permission::JoinCell, &context)
            .is_err());
    }

    #[test]
    fn test_authorization_controller_user_roles() {
        let controller = AuthorizationController::with_default_policy();

        let mut roles = HashSet::new();
        roles.insert(Role::Commander);

        let entity = AuthenticatedEntity::User(UserIdentityInfo {
            username: "commander_alpha".to_string(),
            roles,
        });

        let context = AuthorizationContext::for_cell("test-cell");

        // Commander should be able to approve formation
        assert!(controller
            .check_permission(&entity, Permission::ApproveFormation, &context)
            .is_ok());

        // Commander should be able to form platoon
        assert!(controller
            .check_permission(&entity, Permission::FormPlatoon, &context)
            .is_ok());

        // Commander should NOT be able to manage keys (admin only)
        assert!(controller
            .check_permission(&entity, Permission::ManageKeys, &context)
            .is_err());
    }

    #[test]
    fn test_get_roles_returns_correct_roles() {
        let controller = AuthorizationController::with_default_policy();
        let device_id = test_device_id();
        let device_hex = device_id.to_hex();

        let entity = AuthenticatedEntity::from_device_id(device_id);

        // As leader
        let membership = CellMembershipContext::new(Some(device_hex.clone()), HashSet::new());
        let context = AuthorizationContext::for_cell("test-cell").with_membership(membership);
        let roles = controller.get_roles(&entity, &context);
        assert!(roles.contains(&Role::Leader));
        assert!(!roles.contains(&Role::Member));

        // As member
        let mut members = HashSet::new();
        members.insert(device_hex);
        let membership = CellMembershipContext::new(Some("other".to_string()), members);
        let context = AuthorizationContext::for_cell("test-cell").with_membership(membership);
        let roles = controller.get_roles(&entity, &context);
        assert!(roles.contains(&Role::Member));
        assert!(!roles.contains(&Role::Leader));
    }

    #[test]
    fn test_permission_denied_error_contains_details() {
        let controller = AuthorizationController::with_default_policy();
        let device_id = test_device_id();

        let entity = AuthenticatedEntity::from_device_id(device_id);
        let context = AuthorizationContext::system();

        let result = controller.check_permission(&entity, Permission::ConfigureNetwork, &context);
        assert!(result.is_err());

        if let Err(SecurityError::PermissionDenied {
            permission,
            entity_id,
            ..
        }) = result
        {
            assert_eq!(permission, "ConfigureNetwork");
            assert!(!entity_id.is_empty());
        } else {
            panic!("Expected PermissionDenied error");
        }
    }
}
