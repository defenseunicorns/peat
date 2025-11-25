//! End-to-End Security Tests for HIVE Protocol
//!
//! These tests validate:
//! - Device authentication via challenge-response
//! - Role-based authorization for cell operations
//! - Multi-device mesh authentication
//!
//! # Test Architecture
//!
//! 1. Create multiple device identities with keypairs
//! 2. Test authentication flows between devices
//! 3. Validate RBAC permission checks for different roles
//! 4. Test authorization in context of cell membership

use hive_protocol::security::{
    AuthenticatedEntity, AuthorizationContext, AuthorizationController, CellMembershipContext,
    DeviceAuthenticator, DeviceKeypair, LocalUserStore, MilitaryRank, OrganizationUnit, Permission,
    Role, SecurityClearance, UserAuthenticator, UserIdentity,
};
use std::collections::HashSet;

/// Test: Basic device keypair generation and identity
#[test]
fn test_device_identity_generation() {
    let keypair1 = DeviceKeypair::generate();
    let keypair2 = DeviceKeypair::generate();

    // Each device gets a unique ID
    assert_ne!(keypair1.device_id(), keypair2.device_id());

    // Device IDs are deterministic from keypairs
    let id1_a = keypair1.device_id();
    let id1_b = keypair1.device_id();
    assert_eq!(id1_a, id1_b);

    println!("Device 1 ID: {}", keypair1.device_id().to_hex());
    println!("Device 2 ID: {}", keypair2.device_id().to_hex());
}

/// Test: Full mutual authentication between two devices
#[test]
fn test_mutual_device_authentication() {
    // Create two devices
    let keypair_a = DeviceKeypair::generate();
    let keypair_b = DeviceKeypair::generate();

    let auth_a = DeviceAuthenticator::new(keypair_a);
    let auth_b = DeviceAuthenticator::new(keypair_b);

    // === Round 1: A authenticates B ===

    // A generates challenge for B
    let challenge_for_b = auth_a.generate_challenge();
    assert_eq!(challenge_for_b.nonce.len(), 32);

    // B responds to A's challenge
    let response_from_b = auth_b
        .respond_to_challenge(&challenge_for_b)
        .expect("B should respond to challenge");

    // A verifies B's response
    let verified_b_id = auth_a
        .verify_response(&response_from_b)
        .expect("A should verify B's response");
    assert_eq!(verified_b_id, auth_b.device_id());

    // === Round 2: B authenticates A ===

    // B generates challenge for A
    let challenge_for_a = auth_b.generate_challenge();

    // A responds to B's challenge
    let response_from_a = auth_a
        .respond_to_challenge(&challenge_for_a)
        .expect("A should respond to challenge");

    // B verifies A's response
    let verified_a_id = auth_b
        .verify_response(&response_from_a)
        .expect("B should verify A's response");
    assert_eq!(verified_a_id, auth_a.device_id());

    // Both devices now have verified each other
    assert!(auth_a.is_verified(&verified_b_id));
    assert!(auth_b.is_verified(&verified_a_id));

    println!("Mutual authentication successful!");
    println!("  A verified B: {}", verified_b_id.to_hex());
    println!("  B verified A: {}", verified_a_id.to_hex());
}

/// Test: 3-node mesh authentication (A <-> B <-> C)
#[test]
fn test_three_node_mesh_authentication() {
    // Create three devices
    let keypair_a = DeviceKeypair::generate();
    let keypair_b = DeviceKeypair::generate();
    let keypair_c = DeviceKeypair::generate();

    let auth_a = DeviceAuthenticator::new(keypair_a);
    let auth_b = DeviceAuthenticator::new(keypair_b);
    let auth_c = DeviceAuthenticator::new(keypair_c);

    // Helper to do mutual auth
    fn mutual_auth(
        auth1: &DeviceAuthenticator,
        auth2: &DeviceAuthenticator,
    ) -> (
        hive_protocol::security::DeviceId,
        hive_protocol::security::DeviceId,
    ) {
        // 1 authenticates 2
        let challenge = auth1.generate_challenge();
        let response = auth2.respond_to_challenge(&challenge).unwrap();
        let id2 = auth1.verify_response(&response).unwrap();

        // 2 authenticates 1
        let challenge = auth2.generate_challenge();
        let response = auth1.respond_to_challenge(&challenge).unwrap();
        let id1 = auth2.verify_response(&response).unwrap();

        (id1, id2)
    }

    // A <-> B
    let (_, b_from_a) = mutual_auth(&auth_a, &auth_b);
    assert_eq!(b_from_a, auth_b.device_id());

    // B <-> C
    let (_, c_from_b) = mutual_auth(&auth_b, &auth_c);
    assert_eq!(c_from_b, auth_c.device_id());

    // A <-> C (direct, not via B)
    let (_, c_from_a) = mutual_auth(&auth_a, &auth_c);
    assert_eq!(c_from_a, auth_c.device_id());

    // All three devices have full mesh connectivity
    assert_eq!(auth_a.verified_peer_count(), 2); // B and C
    assert_eq!(auth_b.verified_peer_count(), 2); // A and C
    assert_eq!(auth_c.verified_peer_count(), 2); // A and B

    println!("3-node mesh authentication complete:");
    println!("  Device A verified {} peers", auth_a.verified_peer_count());
    println!("  Device B verified {} peers", auth_b.verified_peer_count());
    println!("  Device C verified {} peers", auth_c.verified_peer_count());
}

/// Test: Invalid signature is rejected
#[test]
fn test_invalid_signature_rejected() {
    let keypair_a = DeviceKeypair::generate();
    let keypair_b = DeviceKeypair::generate();

    let auth_a = DeviceAuthenticator::new(keypair_a);
    let auth_b = DeviceAuthenticator::new(keypair_b);

    let challenge = auth_a.generate_challenge();
    let mut response = auth_b.respond_to_challenge(&challenge).unwrap();

    // Corrupt the signature
    response.signature[0] ^= 0xFF;
    response.signature[10] ^= 0xFF;

    // Verification should fail
    let result = auth_a.verify_response(&response);
    assert!(result.is_err());
    println!("Correctly rejected tampered signature");
}

/// Test: RBAC - Leader permissions for cell operations
#[test]
fn test_rbac_leader_permissions() {
    let controller = AuthorizationController::with_default_policy();
    let keypair = DeviceKeypair::generate();
    let device_id = keypair.device_id();
    let device_hex = device_id.to_hex();

    let entity = AuthenticatedEntity::from_device_id(device_id);

    // Create context where this device is the cell leader
    let membership = CellMembershipContext::new(Some(device_hex), HashSet::new());
    let context = AuthorizationContext::for_cell("alpha-cell").with_membership(membership);

    // Leaders should have these permissions
    let leader_permissions = [
        Permission::SetCellObjective,
        Permission::SetCellLeader,
        Permission::WriteCellState,
        Permission::ReadCellState,
        Permission::RequestCapability,
        Permission::DisbandCell,
    ];

    for permission in leader_permissions {
        let result = controller.check_permission(&entity, permission, &context);
        assert!(
            result.is_ok(),
            "Leader should have {} permission",
            permission
        );
    }

    // Leaders should NOT have admin permissions
    let denied_permissions = [Permission::ConfigureNetwork, Permission::ManageKeys];

    for permission in denied_permissions {
        let result = controller.check_permission(&entity, permission, &context);
        assert!(
            result.is_err(),
            "Leader should NOT have {} permission",
            permission
        );
    }

    println!("Leader permission checks passed");
}

/// Test: RBAC - Member permissions (non-leader)
#[test]
fn test_rbac_member_permissions() {
    let controller = AuthorizationController::with_default_policy();
    let keypair = DeviceKeypair::generate();
    let device_id = keypair.device_id();
    let device_hex = device_id.to_hex();

    let entity = AuthenticatedEntity::from_device_id(device_id);

    // Create context where this device is a member (not leader)
    let mut members = HashSet::new();
    members.insert(device_hex);
    let membership = CellMembershipContext::new(Some("leader-device".to_string()), members);
    let context = AuthorizationContext::for_cell("alpha-cell").with_membership(membership);

    // Members should have these permissions
    let member_permissions = [
        Permission::JoinCell,
        Permission::LeaveCell,
        Permission::ReadCellState,
        Permission::WriteNodeState,
        Permission::AdvertiseCapability,
    ];

    for permission in member_permissions {
        let result = controller.check_permission(&entity, permission, &context);
        assert!(
            result.is_ok(),
            "Member should have {} permission",
            permission
        );
    }

    // Members should NOT have leader-only permissions
    let denied_permissions = [
        Permission::SetCellObjective,
        Permission::SetCellLeader,
        Permission::WriteCellState,
    ];

    for permission in denied_permissions {
        let result = controller.check_permission(&entity, permission, &context);
        assert!(
            result.is_err(),
            "Member should NOT have {} permission",
            permission
        );
    }

    println!("Member permission checks passed");
}

/// Test: RBAC - Observer (non-member) has read-only access
#[test]
fn test_rbac_observer_permissions() {
    let controller = AuthorizationController::with_default_policy();
    let keypair = DeviceKeypair::generate();
    let device_id = keypair.device_id();

    let entity = AuthenticatedEntity::from_device_id(device_id);

    // Create context where this device is neither leader nor member
    let membership = CellMembershipContext::new(Some("some-leader".to_string()), HashSet::new());
    let context = AuthorizationContext::for_cell("alpha-cell").with_membership(membership);

    // Observers can read
    assert!(controller
        .check_permission(&entity, Permission::ReadCellState, &context)
        .is_ok());
    assert!(controller
        .check_permission(&entity, Permission::ReadNodeState, &context)
        .is_ok());
    assert!(controller
        .check_permission(&entity, Permission::ReadTelemetry, &context)
        .is_ok());

    // Observers cannot write or command
    assert!(controller
        .check_permission(&entity, Permission::WriteCellState, &context)
        .is_err());
    assert!(controller
        .check_permission(&entity, Permission::SetCellObjective, &context)
        .is_err());
    assert!(controller
        .check_permission(&entity, Permission::JoinCell, &context)
        .is_err());

    println!("Observer permission checks passed");
}

/// Test: Role-based cell operation authorization scenario
#[test]
fn test_cell_operation_authorization_scenario() {
    let controller = AuthorizationController::with_default_policy();

    // Setup: Create a cell with leader and 2 members
    let leader_keypair = DeviceKeypair::generate();
    let member1_keypair = DeviceKeypair::generate();
    let member2_keypair = DeviceKeypair::generate();
    let outsider_keypair = DeviceKeypair::generate();

    let leader_hex = leader_keypair.device_id().to_hex();
    let member1_hex = member1_keypair.device_id().to_hex();
    let member2_hex = member2_keypair.device_id().to_hex();

    let mut members = HashSet::new();
    members.insert(leader_hex.clone());
    members.insert(member1_hex.clone());
    members.insert(member2_hex.clone());

    let membership = CellMembershipContext::new(Some(leader_hex.clone()), members);
    let context = AuthorizationContext::for_cell("bravo-cell").with_membership(membership);

    // Create entities
    let leader_entity = AuthenticatedEntity::from_device_id(leader_keypair.device_id());
    let member1_entity = AuthenticatedEntity::from_device_id(member1_keypair.device_id());
    let outsider_entity = AuthenticatedEntity::from_device_id(outsider_keypair.device_id());

    // Scenario: Leader sets a new cell objective
    println!("\n--- Scenario: Set Cell Objective ---");

    let result =
        controller.check_permission(&leader_entity, Permission::SetCellObjective, &context);
    println!(
        "Leader sets objective: {}",
        if result.is_ok() { "ALLOWED" } else { "DENIED" }
    );
    assert!(result.is_ok());

    let result =
        controller.check_permission(&member1_entity, Permission::SetCellObjective, &context);
    println!(
        "Member sets objective: {}",
        if result.is_ok() { "ALLOWED" } else { "DENIED" }
    );
    assert!(result.is_err());

    let result =
        controller.check_permission(&outsider_entity, Permission::SetCellObjective, &context);
    println!(
        "Outsider sets objective: {}",
        if result.is_ok() { "ALLOWED" } else { "DENIED" }
    );
    assert!(result.is_err());

    // Scenario: Member advertises capability
    println!("\n--- Scenario: Advertise Capability ---");

    let result =
        controller.check_permission(&member1_entity, Permission::AdvertiseCapability, &context);
    println!(
        "Member advertises capability: {}",
        if result.is_ok() { "ALLOWED" } else { "DENIED" }
    );
    assert!(result.is_ok());

    let result =
        controller.check_permission(&outsider_entity, Permission::AdvertiseCapability, &context);
    println!(
        "Outsider advertises capability: {}",
        if result.is_ok() { "ALLOWED" } else { "DENIED" }
    );
    assert!(result.is_err());

    // Scenario: Read cell state (everyone can read per trust model)
    println!("\n--- Scenario: Read Cell State ---");

    for (name, entity) in [
        ("Leader", &leader_entity),
        ("Member", &member1_entity),
        ("Outsider", &outsider_entity),
    ] {
        let result = controller.check_permission(entity, Permission::ReadCellState, &context);
        println!(
            "{} reads cell state: {}",
            name,
            if result.is_ok() { "ALLOWED" } else { "DENIED" }
        );
        // Per trust model: all mesh members can read (outsider is observer)
        assert!(result.is_ok());
    }

    println!("\nCell operation authorization scenario completed successfully!");
}

/// Test: Permission error includes helpful context
#[test]
fn test_permission_denied_error_details() {
    let controller = AuthorizationController::with_default_policy();
    let keypair = DeviceKeypair::generate();
    let device_id = keypair.device_id();

    let entity = AuthenticatedEntity::from_device_id(device_id);
    let context = AuthorizationContext::system();

    // Observer trying to configure network (admin only)
    let result = controller.check_permission(&entity, Permission::ConfigureNetwork, &context);

    match result {
        Err(hive_protocol::security::SecurityError::PermissionDenied {
            permission,
            entity_id,
            roles,
        }) => {
            assert_eq!(permission, "ConfigureNetwork");
            assert!(!entity_id.is_empty());
            assert!(roles.contains(&"Observer".to_string()));
            println!("Permission denied error includes:");
            println!("  Permission: {}", permission);
            println!("  Entity: {}", entity_id);
            println!("  Roles: {:?}", roles);
        }
        _ => panic!("Expected PermissionDenied error"),
    }
}

// ============================================================================
// User Authentication E2E Tests (Phase 3: #162)
// ============================================================================

/// Test: Human commander authenticates and approves cell formation
#[test]
fn test_commander_authentication_and_approval() {
    // Setup: Create a local user store and authenticator
    let store = LocalUserStore::new();
    let authenticator = UserAuthenticator::new(Box::new(store));

    // Register a mission commander
    let commander_identity = UserIdentity::builder("alpha_6")
        .display_name("CPT John Smith")
        .rank(MilitaryRank::Captain)
        .clearance(SecurityClearance::Secret)
        .unit(OrganizationUnit::new("1st Plt", "A Co"))
        .role(Role::Commander)
        .build();

    // Register the commander (returns TOTP secret for authenticator app)
    let totp_secret = authenticator
        .register_user("alpha_6", "tactical_password_123", commander_identity)
        .expect("Should register commander");

    println!("Commander registered. TOTP secret: {}", totp_secret);

    // Authenticate the commander (using password-only for test)
    let session = authenticator
        .authenticate_password_only("alpha_6", "tactical_password_123")
        .expect("Commander should authenticate");

    println!("Commander authenticated. Session: {}", session.session_id);
    println!(
        "  Identity: {} ({})",
        session.identity.display_name, session.identity.rank
    );
    println!("  Clearance: {}", session.identity.clearance);
    println!("  Unit: {}", session.identity.unit);
    assert!(!session.is_expired());

    // Validate session
    let validated_identity = authenticator
        .validate_session(&session.session_id)
        .expect("Session should be valid");
    assert_eq!(validated_identity.username, "alpha_6");

    // Check commander has approval permission
    let controller = AuthorizationController::with_default_policy();
    let mut roles = HashSet::new();
    roles.insert(Role::Commander);

    let commander_entity = AuthenticatedEntity::User(hive_protocol::security::UserIdentityInfo {
        username: validated_identity.username.clone(),
        roles,
    });

    let context = AuthorizationContext::for_cell("bravo-cell");

    // Commander should be able to approve formation
    let result =
        controller.check_permission(&commander_entity, Permission::ApproveFormation, &context);
    assert!(result.is_ok(), "Commander should approve cell formation");
    println!("Commander approved formation for bravo-cell: ALLOWED");

    // Commander should be able to veto commands
    let result = controller.check_permission(&commander_entity, Permission::VetoCommand, &context);
    assert!(result.is_ok(), "Commander should be able to veto");
    println!("Commander veto capability: ALLOWED");

    // Commander should be able to form platoons
    let result = controller.check_permission(&commander_entity, Permission::FormPlatoon, &context);
    assert!(result.is_ok(), "Commander should form platoons");
    println!("Commander form platoon: ALLOWED");

    // Commander should NOT have admin permissions
    let result =
        controller.check_permission(&commander_entity, Permission::ConfigureNetwork, &context);
    assert!(result.is_err(), "Commander should NOT configure network");
    println!("Commander configure network: DENIED (correct)");

    println!("\nCommander authentication and approval scenario complete!");
}

/// Test: Multiple users with different roles and clearances
#[test]
fn test_multi_user_authorization_hierarchy() {
    let store = LocalUserStore::new();
    let authenticator = UserAuthenticator::new(Box::new(store));
    let controller = AuthorizationController::with_default_policy();

    // Register multiple users with different roles
    let users = [
        (
            "cmd_alpha",
            "commander_pass",
            UserIdentity::builder("cmd_alpha")
                .display_name("MAJ Williams")
                .rank(MilitaryRank::Major)
                .clearance(SecurityClearance::TopSecret)
                .unit(OrganizationUnit::new("HQ", "Bn HQ"))
                .role(Role::Commander)
                .build(),
        ),
        (
            "admin_ops",
            "admin_pass",
            UserIdentity::builder("admin_ops")
                .display_name("SSG Martinez")
                .rank(MilitaryRank::StaffSergeant)
                .clearance(SecurityClearance::Secret)
                .unit(OrganizationUnit::new("S6", "Bn HQ"))
                .role(Role::Admin)
                .build(),
        ),
        (
            "observer_1",
            "observer_pass",
            UserIdentity::builder("observer_1")
                .display_name("CIV Analyst")
                .rank(MilitaryRank::Civilian)
                .clearance(SecurityClearance::Secret)
                .unit(OrganizationUnit::top_level("Contractor"))
                .role(Role::Observer)
                .build(),
        ),
    ];

    // Register and authenticate all users
    let mut sessions = Vec::new();
    for (username, password, identity) in users {
        authenticator
            .register_user(username, password, identity)
            .unwrap();
        let session = authenticator
            .authenticate_password_only(username, password)
            .unwrap();
        sessions.push(session);
    }

    assert_eq!(authenticator.active_session_count(), 3);
    println!("Registered and authenticated 3 users");

    let context = AuthorizationContext::for_cell("test-cell");

    // Check permissions for each user
    for session in &sessions {
        let entity = AuthenticatedEntity::User(hive_protocol::security::UserIdentityInfo {
            username: session.identity.username.clone(),
            roles: session.identity.roles.clone(),
        });

        let can_approve = controller
            .check_permission(&entity, Permission::ApproveFormation, &context)
            .is_ok();
        let can_admin = controller
            .check_permission(&entity, Permission::ConfigureNetwork, &context)
            .is_ok();
        let can_read = controller
            .check_permission(&entity, Permission::ReadCellState, &context)
            .is_ok();

        println!(
            "User: {} ({}) - Approve: {}, Admin: {}, Read: {}",
            session.identity.username,
            session
                .identity
                .roles
                .iter()
                .next()
                .unwrap_or(&Role::Observer),
            can_approve,
            can_admin,
            can_read
        );
    }

    // Verify expected permissions
    // Commander can approve but not admin
    let cmd_entity = AuthenticatedEntity::User(hive_protocol::security::UserIdentityInfo {
        username: sessions[0].identity.username.clone(),
        roles: sessions[0].identity.roles.clone(),
    });
    assert!(controller
        .check_permission(&cmd_entity, Permission::ApproveFormation, &context)
        .is_ok());
    assert!(controller
        .check_permission(&cmd_entity, Permission::ConfigureNetwork, &context)
        .is_err());

    // Admin can configure but not approve
    let admin_entity = AuthenticatedEntity::User(hive_protocol::security::UserIdentityInfo {
        username: sessions[1].identity.username.clone(),
        roles: sessions[1].identity.roles.clone(),
    });
    assert!(controller
        .check_permission(&admin_entity, Permission::ConfigureNetwork, &context)
        .is_ok());
    assert!(controller
        .check_permission(&admin_entity, Permission::ApproveFormation, &context)
        .is_err());

    // Observer can only read
    let obs_entity = AuthenticatedEntity::User(hive_protocol::security::UserIdentityInfo {
        username: sessions[2].identity.username.clone(),
        roles: sessions[2].identity.roles.clone(),
    });
    assert!(controller
        .check_permission(&obs_entity, Permission::ReadCellState, &context)
        .is_ok());
    assert!(controller
        .check_permission(&obs_entity, Permission::WriteCellState, &context)
        .is_err());

    println!("\nMulti-user authorization hierarchy test complete!");
}

/// Test: Session lifecycle management
#[test]
fn test_session_lifecycle() {
    let store = LocalUserStore::new();
    let authenticator = UserAuthenticator::new(Box::new(store));

    // Register a user
    let identity = UserIdentity::builder("session_test_user")
        .rank(MilitaryRank::Sergeant)
        .role(Role::Member)
        .build();

    authenticator
        .register_user("session_test_user", "password123", identity)
        .unwrap();

    // Create multiple sessions
    let session1 = authenticator
        .authenticate_password_only("session_test_user", "password123")
        .unwrap();
    let session2 = authenticator
        .authenticate_password_only("session_test_user", "password123")
        .unwrap();

    assert_eq!(authenticator.active_session_count(), 2);
    assert_ne!(session1.session_id, session2.session_id);
    println!("Created 2 sessions for same user");

    // Validate both sessions
    assert!(authenticator.validate_session(&session1.session_id).is_ok());
    assert!(authenticator.validate_session(&session2.session_id).is_ok());

    // Invalidate one session
    authenticator.invalidate_session(&session1.session_id);
    assert_eq!(authenticator.active_session_count(), 1);
    assert!(authenticator
        .validate_session(&session1.session_id)
        .is_err());
    assert!(authenticator.validate_session(&session2.session_id).is_ok());
    println!("Invalidated session 1, session 2 still valid");

    // Invalidate all sessions for user
    authenticator.invalidate_user_sessions("session_test_user");
    assert_eq!(authenticator.active_session_count(), 0);
    println!("Invalidated all sessions for user");

    println!("\nSession lifecycle test complete!");
}

/// Test: Bind user session to device
#[test]
fn test_user_session_device_binding() {
    let store = LocalUserStore::new();
    let authenticator = UserAuthenticator::new(Box::new(store));

    // Register a user (operator using a C2 tablet)
    let identity = UserIdentity::builder("tablet_operator")
        .display_name("SGT Operator")
        .rank(MilitaryRank::Sergeant)
        .role(Role::Member)
        .build();

    authenticator
        .register_user("tablet_operator", "password", identity)
        .unwrap();

    // Authenticate
    let session = authenticator
        .authenticate_password_only("tablet_operator", "password")
        .unwrap();
    assert!(session.device_id.is_none());
    println!("User authenticated without device binding");

    // Bind to a device (C2 tablet)
    let tablet_keypair = DeviceKeypair::generate();
    let tablet_id = tablet_keypair.device_id();

    authenticator
        .bind_session_to_device(&session.session_id, tablet_id)
        .unwrap();

    // Verify binding
    let updated_session = authenticator.get_session(&session.session_id).unwrap();
    assert!(updated_session.device_id.is_some());
    assert_eq!(updated_session.device_id.unwrap(), tablet_id);
    println!("Session bound to device: {}", tablet_id.to_hex());

    println!("\nUser session device binding test complete!");
}
