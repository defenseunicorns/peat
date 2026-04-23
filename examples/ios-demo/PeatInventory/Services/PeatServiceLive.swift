import Foundation

/// Live implementation of PeatServiceProtocol that wraps the real peat-ffi UniFFI bindings.
/// Requires the PeatFFI.xcframework to be linked and the generated Swift bindings present.
///
/// To use this:
/// 1. Build peat-ffi for iOS targets (see CLAUDE.md build instructions)
/// 2. Generate Swift bindings via uniffi-bindgen
/// 3. Link PeatFFI.xcframework to the Xcode project
/// 4. Uncomment the import and implementation below
///
/// The FFI exposes:
/// - PeatNode: main object with put_document/get_document/list_documents/delete_document
/// - DocumentCallback: protocol for receiving change notifications
/// - NodeConfig: configuration for creating nodes
/// - PeerInfo, SyncStats: status types

// import PeatFFI  // Uncomment when FFI framework is available

final class PeatServiceLive: PeatServiceProtocol, @unchecked Sendable {
    // private var node: PeatNode?
    // private var subscription: SubscriptionHandle?

    private var changeHandler: (@Sendable ([InventoryItem]) -> Void)?
    private var items: [UUID: InventoryItem] = [:]
    private let collectionName = "inventory"

    func start() async throws {
        // When FFI is ready:
        // let config = NodeConfig(
        //     appId: "peat-inventory",
        //     sharedKey: "demo-shared-key",
        //     bindAddress: nil,
        //     storagePath: FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
        //         .appendingPathComponent("peat-data").path,
        //     transport: TransportConfigFFI(
        //         enableBle: true,
        //         bleMeshId: "peat-inv",
        //         blePowerProfile: nil,
        //         transportPreference: nil,
        //         collectionRoutesJson: nil
        //     )
        // )
        // node = try createNode(config: config)
        // try node?.startSync()
        //
        // subscription = try node?.subscribe(callback: DocumentCallbackImpl { [weak self] change in
        //     Task { await self?.handleRemoteChange(change) }
        // })

        fatalError("PeatServiceLive requires peat-ffi framework. Use PeatServiceMock instead.")
    }

    func stop() async throws {
        // subscription?.cancel()
        // try node?.stopSync()
    }

    func getAllItems() async -> [InventoryItem] {
        // guard let node else { return [] }
        // let docIds = try? node.listDocuments(collection: collectionName)
        // return (docIds ?? []).compactMap { docId in
        //     guard let json = try? node.getDocument(collection: collectionName, docId: docId),
        //           let data = json.data(using: .utf8),
        //           let item = try? JSONDecoder.peatDecoder.decode(InventoryItem.self, from: data) else {
        //         return nil
        //     }
        //     return item
        // }.sorted { $0.nomenclature < $1.nomenclature }
        return []
    }

    func putItem(_ item: InventoryItem) async throws {
        // guard let node else { return }
        // let data = try JSONEncoder.peatEncoder.encode(item)
        // let json = String(data: data, encoding: .utf8)!
        // try node.putDocument(collection: collectionName, docId: item.id.uuidString, jsonData: json)
        // try node.syncDocument(collection: collectionName, docId: item.id.uuidString)
    }

    func deleteItem(id: UUID) async throws {
        // guard let node else { return }
        // try node.deleteDocument(collection: collectionName, docId: id.uuidString)
    }

    func getDiscoveredPeers() async -> [PeerInfo] { [] }
    func getCellMembers() async -> [PeerInfo] { [] }

    func onRemoteChange(_ handler: @escaping @Sendable ([InventoryItem]) -> Void) {
        changeHandler = handler
    }

    func getActiveTransports() async -> [TransportType] { [] }
    func getSyncStatus() async -> SyncStatus { .offline }
    func getLastSyncTime() async -> Date? { nil }
    func getCellInfo() async -> CellInfo? { nil }
    func getSyncEvents() async -> [SyncEvent] { [] }
    func setNodeName(_ name: String) async {}
    func getNodeName() async -> String { "Live Node" }
    func getNodeId() async -> String { "live-node" }

    // MARK: - FFI Callback (uncomment when available)

    // private func handleRemoteChange(_ change: DocumentChange) async {
    //     if change.collection == collectionName {
    //         let allItems = await getAllItems()
    //         changeHandler?(allItems)
    //     }
    // }
}

// Uncomment when FFI is available:
// class DocumentCallbackImpl: DocumentCallback {
//     let handler: (DocumentChange) -> Void
//     init(handler: @escaping (DocumentChange) -> Void) { self.handler = handler }
//     func onChange(change: DocumentChange) { handler(change) }
//     func onError(message: String) { print("Peat FFI error: \(message)") }
// }
