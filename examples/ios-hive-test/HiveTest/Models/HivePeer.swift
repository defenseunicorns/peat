//
//  HivePeer.swift
//  HiveTest
//
//  Model representing a peer in the HIVE mesh network
//

import Foundation

/// Represents a peer in the HIVE mesh network
struct HivePeer: Identifiable, Equatable, Hashable {
    /// Unique identifier (CoreBluetooth peripheral UUID)
    let identifier: String

    /// HIVE node ID (32-bit)
    let nodeId: UInt32

    /// Mesh ID this peer belongs to (e.g., "DEMO", "ALFA")
    let meshId: String?

    /// Raw advertised name from BLE
    let advertisedName: String?

    /// Whether this peer is currently connected
    var isConnected: Bool

    /// Last seen RSSI value (dBm)
    var rssi: Int8

    /// Last time this peer was seen
    var lastSeen: Date

    /// SwiftUI identifier
    var id: String { identifier }

    /// Display name - shows mesh ID if available
    var displayName: String {
        if let meshId = meshId {
            return "HIVE_\(meshId)-\(String(format: "%08X", nodeId))"
        } else {
            return String(format: "HIVE-%08X", nodeId)
        }
    }

    /// Signal strength description
    var signalStrength: SignalStrength {
        switch rssi {
        case -50...0:
            return .excellent
        case -70..<(-50):
            return .good
        case -85..<(-70):
            return .fair
        default:
            return .weak
        }
    }
}

/// Signal strength categories
enum SignalStrength: String {
    case excellent = "Excellent"
    case good = "Good"
    case fair = "Fair"
    case weak = "Weak"

    var color: String {
        switch self {
        case .excellent: return "green"
        case .good: return "blue"
        case .fair: return "yellow"
        case .weak: return "red"
        }
    }

    var iconName: String {
        switch self {
        case .excellent: return "wifi"
        case .good: return "wifi"
        case .fair: return "wifi.exclamationmark"
        case .weak: return "wifi.slash"
        }
    }
}

/// Event types that can be sent/received in the HIVE mesh
enum HiveEventType: String, CaseIterable {
    case none = "None"
    case emergency = "Emergency"
    case ack = "ACK"
    case needAssist = "Need Assist"
    case ping = "Ping"
    case heartbeat = "Heartbeat"
}

/// ACK tracking for alerts
struct AckStatus {
    var pendingAcks: [UInt32: Bool] = [:] // nodeId -> hasAcked
    var emergencySourceNodeId: UInt32?

    var isActive: Bool {
        emergencySourceNodeId != nil
    }

    var ackedNodes: [UInt32] {
        pendingAcks.filter { $0.value }.map { $0.key }
    }

    var waitingNodes: [UInt32] {
        pendingAcks.filter { !$0.value }.map { $0.key }
    }

    var allAcked: Bool {
        !pendingAcks.isEmpty && pendingAcks.values.allSatisfy { $0 }
    }

    mutating func reset() {
        pendingAcks.removeAll()
        emergencySourceNodeId = nil
    }
}
