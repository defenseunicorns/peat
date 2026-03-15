//! Secure Node Example
//!
//! Demonstrates the Peat security framework for device authentication
//! and RBAC authorization.
//!
//! # What This Shows
//!
//! 1. **Device Authentication**: Ed25519 keypair generation and challenge-response
//! 2. **RBAC Authorization**: Role-based permission checking for cell operations
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --example secure_node -p peat-protocol
//! ```
//!
//! # Integration with peat-sim
//!
//! To add security to your simulation nodes:
//!
//! 1. Generate device identity at startup using `DeviceKeypair::generate()`
//! 2. Use `DeviceAuthenticator` for challenge-response with peers
//! 3. Check permissions with `AuthorizationController` before operations

use peat_protocol::security::{
    AuthenticatedEntity, AuthorizationContext, AuthorizationController, CellMembershipContext,
    DeviceAuthenticator, DeviceKeypair, Permission,
};
use std::collections::HashSet;

fn main() {
    println!("=== Peat Security Framework Demo ===\n");

    demo_device_authentication();
    demo_rbac_authorization();
    demo_integrated_flow();

    println!("\n=== Security Demo Complete ===");
}

/// Demonstrates device identity and mutual authentication
fn demo_device_authentication() {
    println!("--- 1. Device Authentication ---\n");

    // Generate keypairs for two devices
    let keypair_a = DeviceKeypair::generate();
    let keypair_b = DeviceKeypair::generate();

    println!("Device A ID: {}", keypair_a.device_id().to_hex());
    println!("Device B ID: {}", keypair_b.device_id().to_hex());

    // Create authenticators
    let auth_a = DeviceAuthenticator::new(keypair_a);
    let auth_b = DeviceAuthenticator::new(keypair_b);

    // === Round 1: A authenticates B ===
    println!("\n--- A authenticates B ---");

    // A generates challenge for B
    let challenge_for_b = auth_a.generate_challenge();
    println!(
        "A sends challenge (nonce: {} bytes)",
        challenge_for_b.nonce.len()
    );

    // B responds to A's challenge
    let response_from_b = auth_b
        .respond_to_challenge(&challenge_for_b)
        .expect("B should respond to challenge");
    println!(
        "B sends signed response ({} bytes)",
        response_from_b.signature.len()
    );

    // A verifies B's response
    let verified_b_id = auth_a
        .verify_response(&response_from_b)
        .expect("A should verify B's response");
    println!("A verified B: {} ✓", verified_b_id.to_hex());

    // === Round 2: B authenticates A ===
    println!("\n--- B authenticates A ---");

    // B generates challenge for A
    let challenge_for_a = auth_b.generate_challenge();
    println!(
        "B sends challenge (nonce: {} bytes)",
        challenge_for_a.nonce.len()
    );

    // A responds to B's challenge
    let response_from_a = auth_a
        .respond_to_challenge(&challenge_for_a)
        .expect("A should respond to challenge");
    println!(
        "A sends signed response ({} bytes)",
        response_from_a.signature.len()
    );

    // B verifies A's response
    let verified_a_id = auth_b
        .verify_response(&response_from_a)
        .expect("B should verify A's response");
    println!("B verified A: {} ✓", verified_a_id.to_hex());

    // Both devices now have verified each other
    println!("\nMutual authentication complete:");
    println!("  A verified {} peers", auth_a.verified_peer_count());
    println!("  B verified {} peers", auth_b.verified_peer_count());

    println!();
}

/// Demonstrates role-based access control
fn demo_rbac_authorization() {
    println!("--- 2. RBAC Authorization ---\n");

    // Create authorization controller with default policy
    let controller = AuthorizationController::with_default_policy();

    // Create device identities
    let leader_keypair = DeviceKeypair::generate();
    let member_keypair = DeviceKeypair::generate();
    let observer_keypair = DeviceKeypair::generate();

    let leader_id = leader_keypair.device_id();
    let member_id = member_keypair.device_id();
    let observer_id = observer_keypair.device_id();

    println!("Created 3 device identities:");
    println!("  Leader:   {}", leader_id.to_hex());
    println!("  Member:   {}", member_id.to_hex());
    println!("  Observer: {}", observer_id.to_hex());

    // Create authenticated entities
    let leader = AuthenticatedEntity::from_device_id(leader_id);
    let member = AuthenticatedEntity::from_device_id(member_id);
    let observer = AuthenticatedEntity::from_device_id(observer_id);

    // Create authorization context where leader is the cell leader
    let mut cell_members = HashSet::new();
    cell_members.insert(leader_id.to_hex());
    cell_members.insert(member_id.to_hex());

    let membership = CellMembershipContext::new(Some(leader_id.to_hex()), cell_members);
    let context = AuthorizationContext::for_cell("alpha-squad").with_membership(membership);

    println!("\nCell context:");
    println!("  Cell ID: alpha-squad");
    println!("  Leader: {}", leader_id.to_hex());

    // Test permissions for Leader (cell leader has elevated permissions)
    println!("\nLeader permissions:");
    check_permission(
        &controller,
        &leader,
        Permission::SetCellObjective,
        &context,
        "SetCellObjective",
    );
    check_permission(
        &controller,
        &leader,
        Permission::SetCellLeader,
        &context,
        "SetCellLeader",
    );
    check_permission(
        &controller,
        &leader,
        Permission::WriteCellState,
        &context,
        "WriteCellState",
    );
    check_permission(
        &controller,
        &leader,
        Permission::DisbandCell,
        &context,
        "DisbandCell",
    );

    // Test permissions for Member (has limited permissions)
    println!("\nMember permissions:");
    check_permission(
        &controller,
        &member,
        Permission::ReadCellState,
        &context,
        "ReadCellState",
    );
    check_permission(
        &controller,
        &member,
        Permission::WriteCellState,
        &context,
        "WriteCellState",
    );
    check_permission(
        &controller,
        &member,
        Permission::SetCellLeader,
        &context,
        "SetCellLeader",
    );

    // Test permissions for Observer (read-only)
    // Create context without observer in cell members
    let observer_context = AuthorizationContext::for_cell("alpha-squad").with_membership(
        CellMembershipContext::new(Some(leader_id.to_hex()), HashSet::new()),
    );

    println!("\nObserver permissions (not a cell member):");
    check_permission(
        &controller,
        &observer,
        Permission::ReadCellState,
        &observer_context,
        "ReadCellState",
    );
    check_permission(
        &controller,
        &observer,
        Permission::WriteCellState,
        &observer_context,
        "WriteCellState",
    );

    println!();
}

fn check_permission(
    controller: &AuthorizationController,
    entity: &AuthenticatedEntity,
    permission: Permission,
    context: &AuthorizationContext,
    name: &str,
) {
    let result = controller.check_permission(entity, permission, context);
    let status = if result.is_ok() {
        "ALLOWED ✓"
    } else {
        "DENIED ✗"
    };
    println!("  {}: {}", name, status);
}

/// Demonstrates an integrated security flow
fn demo_integrated_flow() {
    println!("--- 3. Integrated Security Flow ---\n");
    println!("Scenario: New device joining an existing cell\n");

    // Step 1: Generate device identity
    let new_device = DeviceKeypair::generate();
    let new_device_id = new_device.device_id();
    println!(
        "1. New device generated identity: {}",
        new_device_id.to_hex()
    );

    // Step 2: Existing cell leader
    let leader_device = DeviceKeypair::generate();
    let leader_id = leader_device.device_id();
    println!("2. Cell leader identity: {}", leader_id.to_hex());

    // Step 3: Mutual authentication
    let new_auth = DeviceAuthenticator::new(new_device);
    let leader_auth = DeviceAuthenticator::new(leader_device);

    // New device authenticates to leader
    let challenge = leader_auth.generate_challenge();
    let response = new_auth.respond_to_challenge(&challenge).unwrap();
    let verified_new = leader_auth.verify_response(&response);

    // Leader authenticates to new device
    let challenge = new_auth.generate_challenge();
    let response = leader_auth.respond_to_challenge(&challenge).unwrap();
    let verified_leader = new_auth.verify_response(&response);

    println!(
        "3. Mutual authentication: {}",
        if verified_new.is_ok() && verified_leader.is_ok() {
            "SUCCESS ✓"
        } else {
            "FAILED ✗"
        }
    );

    // Step 4: Check authorization to join
    let controller = AuthorizationController::with_default_policy();
    let entity = AuthenticatedEntity::from_device_id(new_device_id);

    // Context: cell exists but new device not yet a member
    let membership = CellMembershipContext::new(Some(leader_id.to_hex()), HashSet::new());
    let context = AuthorizationContext::for_cell("alpha-squad").with_membership(membership);

    let can_read = controller.check_permission(&entity, Permission::ReadCellState, &context);
    let can_write = controller.check_permission(&entity, Permission::WriteCellState, &context);

    println!("4. Authorization check (as non-member):");
    println!(
        "   ReadCellState: {}",
        if can_read.is_ok() {
            "ALLOWED"
        } else {
            "DENIED"
        }
    );
    println!(
        "   WriteCellState: {}",
        if can_write.is_ok() {
            "ALLOWED"
        } else {
            "DENIED"
        }
    );

    // Step 5: After joining (device added to cell members)
    let mut members = HashSet::new();
    members.insert(leader_id.to_hex());
    members.insert(new_device_id.to_hex());

    let membership = CellMembershipContext::new(Some(leader_id.to_hex()), members);
    let context = AuthorizationContext::for_cell("alpha-squad").with_membership(membership);

    let can_read = controller.check_permission(&entity, Permission::ReadCellState, &context);
    let can_write = controller.check_permission(&entity, Permission::WriteCellState, &context);

    println!("5. Authorization check (after joining as member):");
    println!(
        "   ReadCellState: {}",
        if can_read.is_ok() {
            "ALLOWED ✓"
        } else {
            "DENIED"
        }
    );
    println!(
        "   WriteCellState: {}",
        if can_write.is_ok() {
            "ALLOWED ✓"
        } else {
            "DENIED"
        }
    );

    println!("\n✓ New device securely joined the cell");
}
