import Foundation

struct PeerInfo: Identifiable, Codable, Hashable, Sendable {
    let id: String
    var name: String
    var nodeId: String
    var addresses: [String]
    var transportType: TransportType
    var signalStrength: SignalStrength
    var lastSeen: Date
    var isCellMember: Bool
}

enum TransportType: String, Codable, CaseIterable, Sendable {
    case wifi = "Wi-Fi"
    case ble = "BLE"
    case quic = "QUIC"

    var sfSymbol: String {
        switch self {
        case .wifi: "wifi"
        case .ble: "antenna.radiowaves.left.and.right"
        case .quic: "bolt.horizontal.fill"
        }
    }
}

enum SignalStrength: String, Codable, Sendable {
    case strong, medium, weak, unknown

    var bars: Int {
        switch self {
        case .strong: 3
        case .medium: 2
        case .weak: 1
        case .unknown: 0
        }
    }
}

enum SyncStatus: String, Sendable {
    case synced = "Synced"
    case syncing = "Syncing"
    case pendingChanges = "Pending Changes"
    case offline = "Offline"
    case error = "Error"

    var sfSymbol: String {
        switch self {
        case .synced: "checkmark.circle.fill"
        case .syncing: "arrow.triangle.2.circlepath"
        case .pendingChanges: "exclamationmark.circle.fill"
        case .offline: "wifi.slash"
        case .error: "xmark.circle.fill"
        }
    }
}

struct CellInfo: Sendable {
    var cellId: String
    var cellName: String
    var memberCount: Int
    var role: CellRole
    var leaderId: String?
}

enum CellRole: String, Sendable {
    case leader = "Leader"
    case member = "Member"
}

struct SyncEvent: Identifiable, Sendable {
    let id: UUID
    let timestamp: Date
    let peerName: String
    let eventType: SyncEventType
    let detail: String
}

enum SyncEventType: String, Sendable {
    case connected = "Connected"
    case disconnected = "Disconnected"
    case syncComplete = "Sync Complete"
    case documentsReceived = "Docs Received"
    case documentsSent = "Docs Sent"
    case transportChanged = "Transport Changed"

    var sfSymbol: String {
        switch self {
        case .connected: "link"
        case .disconnected: "link.badge.plus"
        case .syncComplete: "checkmark.circle"
        case .documentsReceived: "arrow.down.doc"
        case .documentsSent: "arrow.up.doc"
        case .transportChanged: "arrow.triangle.swap"
        }
    }
}
