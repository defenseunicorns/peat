import Foundation
import MultipeerConnectivity

final class PeatServiceMock: NSObject, PeatServiceProtocol, @unchecked Sendable {
    private let persistence = PersistenceService()
    private var items: [UUID: InventoryItem] = [:]
    private var peers: [PeerInfo] = []
    private var syncEvents: [SyncEvent] = []
    private var changeHandler: (@Sendable ([InventoryItem]) -> Void)?
    private var nodeName: String = UIDevice.current.name
    private let nodeId: String = UUID().uuidString.prefix(8).lowercased().description

    // MultipeerConnectivity
    private var peerID: MCPeerID!
    private var session: MCSession!
    private var advertiser: MCNearbyServiceAdvertiser!
    private var browser: MCNearbyServiceBrowser!
    private let serviceType = "peat-inventory"
    private var mcPeerMap: [MCPeerID: String] = [:]

    private var _syncStatus: SyncStatus = .offline
    private var _lastSyncTime: Date?
    private var _activeTransports: [TransportType] = []

    override init() {
        super.init()
    }

    func start() async throws {
        let stored = await persistence.loadItems()
        for item in stored {
            items[item.id] = item
        }

        await MainActor.run {
            self.peerID = MCPeerID(displayName: self.nodeName)
            self.session = MCSession(peer: self.peerID, securityIdentity: nil, encryptionPreference: .required)
            self.session.delegate = self

            self.advertiser = MCNearbyServiceAdvertiser(
                peer: self.peerID,
                discoveryInfo: ["nodeId": self.nodeId],
                serviceType: self.serviceType
            )
            self.advertiser.delegate = self
            self.advertiser.startAdvertisingPeer()

            self.browser = MCNearbyServiceBrowser(peer: self.peerID, serviceType: self.serviceType)
            self.browser.delegate = self
            self.browser.startBrowsingForPeers()
        }

        _activeTransports = [.wifi]
        _syncStatus = .synced
    }

    func stop() async throws {
        await MainActor.run {
            self.advertiser?.stopAdvertisingPeer()
            self.browser?.stopBrowsingForPeers()
            self.session?.disconnect()
        }
        _activeTransports = []
        _syncStatus = .offline
    }

    // MARK: - Inventory CRUD

    func getAllItems() async -> [InventoryItem] {
        Array(items.values).sorted { $0.nomenclature < $1.nomenclature }
    }

    func putItem(_ item: InventoryItem) async throws {
        var mutableItem = item
        mutableItem.lastModified = Date()
        mutableItem.modifiedBy = nodeId
        items[item.id] = mutableItem
        await persistence.saveItems(Array(items.values))
        broadcastChange(mutableItem, type: .upsert)
    }

    func deleteItem(id: UUID) async throws {
        guard let item = items.removeValue(forKey: id) else { return }
        await persistence.saveItems(Array(items.values))
        broadcastChange(item, type: .delete)
    }

    // MARK: - Discovery

    func getDiscoveredPeers() async -> [PeerInfo] { peers }
    func getCellMembers() async -> [PeerInfo] { peers.filter(\.isCellMember) }

    // MARK: - Sync

    func onRemoteChange(_ handler: @escaping @Sendable ([InventoryItem]) -> Void) {
        self.changeHandler = handler
    }

    // MARK: - Status

    func getActiveTransports() async -> [TransportType] { _activeTransports }
    func getSyncStatus() async -> SyncStatus { _syncStatus }
    func getLastSyncTime() async -> Date? { _lastSyncTime }

    func getCellInfo() async -> CellInfo? {
        CellInfo(
            cellId: "CELL-\(nodeId.prefix(4).uppercased())",
            cellName: "Alpha Cell",
            memberCount: peers.filter(\.isCellMember).count + 1,
            role: .leader,
            leaderId: nodeId
        )
    }

    func getSyncEvents() async -> [SyncEvent] { syncEvents }

    func setNodeName(_ name: String) async {
        nodeName = name
        // Restart advertising with new name
        await MainActor.run {
            self.advertiser?.stopAdvertisingPeer()
            self.peerID = MCPeerID(displayName: name)
            self.session = MCSession(peer: self.peerID, securityIdentity: nil, encryptionPreference: .required)
            self.session.delegate = self
            self.advertiser = MCNearbyServiceAdvertiser(
                peer: self.peerID,
                discoveryInfo: ["nodeId": self.nodeId],
                serviceType: self.serviceType
            )
            self.advertiser.delegate = self
            self.advertiser.startAdvertisingPeer()
            self.browser = MCNearbyServiceBrowser(peer: self.peerID, serviceType: self.serviceType)
            self.browser.delegate = self
            self.browser.startBrowsingForPeers()
        }
    }

    func getNodeName() async -> String { nodeName }
    func getNodeId() async -> String { nodeId }

    // MARK: - Private

    private func broadcastChange(_ item: InventoryItem, type: InventoryChangeType) {
        guard let session = session, !session.connectedPeers.isEmpty else { return }

        let change = InventoryChange(item: item, changeType: type)
        struct SyncMessage: Codable {
            let item: InventoryItem
            let isDelete: Bool
        }
        let msg = SyncMessage(item: change.item, isDelete: type == .delete)
        guard let data = try? JSONEncoder.peatEncoder.encode(msg) else { return }
        try? session.send(data, toPeers: session.connectedPeers, with: .reliable)
    }

    private func addSyncEvent(_ event: SyncEvent) {
        syncEvents.insert(event, at: 0)
        if syncEvents.count > 50 { syncEvents = Array(syncEvents.prefix(50)) }
    }
}

// MARK: - MCSessionDelegate

extension PeatServiceMock: MCSessionDelegate {
    func session(_ session: MCSession, peer peerID: MCPeerID, didChange state: MCSessionState) {
        let peerNodeId = mcPeerMap[peerID] ?? peerID.displayName
        switch state {
        case .connected:
            let peer = PeerInfo(
                id: peerNodeId,
                name: peerID.displayName,
                nodeId: peerNodeId,
                addresses: [],
                transportType: .wifi,
                signalStrength: .strong,
                lastSeen: Date(),
                isCellMember: true
            )
            if !peers.contains(where: { $0.nodeId == peerNodeId }) {
                peers.append(peer)
            }
            addSyncEvent(SyncEvent(
                id: UUID(), timestamp: Date(),
                peerName: peerID.displayName,
                eventType: .connected,
                detail: "Peer joined via Wi-Fi"
            ))
            _syncStatus = .syncing
            sendFullState(to: peerID)

        case .notConnected:
            peers.removeAll { $0.nodeId == peerNodeId }
            addSyncEvent(SyncEvent(
                id: UUID(), timestamp: Date(),
                peerName: peerID.displayName,
                eventType: .disconnected,
                detail: "Peer disconnected"
            ))
            if peers.isEmpty { _syncStatus = .synced }

        case .connecting:
            break

        @unknown default:
            break
        }
    }

    func session(_ session: MCSession, didReceive data: Data, fromPeer peerID: MCPeerID) {
        struct SyncMessage: Codable {
            let item: InventoryItem
            let isDelete: Bool
        }

        // Try single item message
        if let msg = try? JSONDecoder.peatDecoder.decode(SyncMessage.self, from: data) {
            if msg.isDelete {
                items.removeValue(forKey: msg.item.id)
            } else {
                let existing = items[msg.item.id]
                if existing == nil || existing!.lastModified < msg.item.lastModified {
                    items[msg.item.id] = msg.item
                }
            }
            Task { await persistence.saveItems(Array(items.values)) }
            _lastSyncTime = Date()
            _syncStatus = .synced
            addSyncEvent(SyncEvent(
                id: UUID(), timestamp: Date(),
                peerName: peerID.displayName,
                eventType: .documentsReceived,
                detail: "Received: \(msg.item.nomenclature)"
            ))
            changeHandler?(Array(items.values))
            return
        }

        // Try full state sync
        if let allItems = try? JSONDecoder.peatDecoder.decode([InventoryItem].self, from: data) {
            for item in allItems {
                let existing = items[item.id]
                if existing == nil || existing!.lastModified < item.lastModified {
                    items[item.id] = item
                }
            }
            Task { await persistence.saveItems(Array(items.values)) }
            _lastSyncTime = Date()
            _syncStatus = .synced
            addSyncEvent(SyncEvent(
                id: UUID(), timestamp: Date(),
                peerName: peerID.displayName,
                eventType: .syncComplete,
                detail: "Full sync: \(allItems.count) items"
            ))
            changeHandler?(Array(items.values))
        }
    }

    func session(_ session: MCSession, didReceive stream: InputStream, withName streamName: String, fromPeer peerID: MCPeerID) {}
    func session(_ session: MCSession, didStartReceivingResourceWithName resourceName: String, fromPeer peerID: MCPeerID, with progress: Progress) {}
    func session(_ session: MCSession, didFinishReceivingResourceWithName resourceName: String, fromPeer peerID: MCPeerID, at localURL: URL?, withError error: Error?) {}

    private func sendFullState(to peerID: MCPeerID) {
        let allItems = Array(items.values)
        guard let data = try? JSONEncoder.peatEncoder.encode(allItems) else { return }
        try? session.send(data, toPeers: [peerID], with: .reliable)
        addSyncEvent(SyncEvent(
            id: UUID(), timestamp: Date(),
            peerName: peerID.displayName,
            eventType: .documentsSent,
            detail: "Sent full state: \(allItems.count) items"
        ))
    }
}

// MARK: - MCNearbyServiceAdvertiserDelegate

extension PeatServiceMock: MCNearbyServiceAdvertiserDelegate {
    func advertiser(_ advertiser: MCNearbyServiceAdvertiser, didReceiveInvitationFromPeer peerID: MCPeerID, withContext context: Data?, invitationHandler: @escaping (Bool, MCSession?) -> Void) {
        if let context = context, let nodeId = String(data: context, encoding: .utf8) {
            mcPeerMap[peerID] = nodeId
        }
        invitationHandler(true, session)
    }
}

// MARK: - MCNearbyServiceBrowserDelegate

extension PeatServiceMock: MCNearbyServiceBrowserDelegate {
    func browser(_ browser: MCNearbyServiceBrowser, foundPeer peerID: MCPeerID, withDiscoveryInfo info: [String: String]?) {
        if let nodeId = info?["nodeId"] {
            mcPeerMap[peerID] = nodeId
        }
        let context = nodeId.data(using: .utf8)
        browser.invitePeer(peerID, to: session, withContext: context, timeout: 10)
    }

    func browser(_ browser: MCNearbyServiceBrowser, lostPeer peerID: MCPeerID) {
        let peerNodeId = mcPeerMap[peerID] ?? peerID.displayName
        peers.removeAll { $0.nodeId == peerNodeId }
    }
}
