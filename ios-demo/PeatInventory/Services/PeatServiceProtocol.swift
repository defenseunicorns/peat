import Foundation

protocol PeatServiceProtocol: AnyObject, Sendable {
    // Lifecycle
    func start() async throws
    func stop() async throws

    // Inventory CRUD
    func getAllItems() async -> [InventoryItem]
    func putItem(_ item: InventoryItem) async throws
    func deleteItem(id: UUID) async throws

    // Discovery
    func getDiscoveredPeers() async -> [PeerInfo]
    func getCellMembers() async -> [PeerInfo]

    // Sync
    func onRemoteChange(_ handler: @escaping @Sendable ([InventoryItem]) -> Void)

    // Status
    func getActiveTransports() async -> [TransportType]
    func getSyncStatus() async -> SyncStatus
    func getLastSyncTime() async -> Date?
    func getCellInfo() async -> CellInfo?
    func getSyncEvents() async -> [SyncEvent]

    // Configuration
    func setNodeName(_ name: String) async
    func getNodeName() async -> String
    func getNodeId() async -> String
}

struct InventoryChange: Sendable {
    let item: InventoryItem
    let changeType: InventoryChangeType
}

enum InventoryChangeType: Sendable {
    case upsert
    case delete
}
