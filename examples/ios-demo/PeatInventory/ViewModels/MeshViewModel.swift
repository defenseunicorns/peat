import Foundation

@Observable
final class MeshViewModel {
    var peers: [PeerInfo] = []
    var cellInfo: CellInfo?
    var syncStatus: SyncStatus = .offline
    var lastSyncTime: Date?
    var activeTransports: [TransportType] = []
    var syncEvents: [SyncEvent] = []
    var nodeId: String = ""
    var nodeName: String = ""

    private let service: PeatServiceProtocol
    private var refreshTask: Task<Void, Never>?

    init(service: PeatServiceProtocol) {
        self.service = service
    }

    func startMonitoring() async {
        await refresh()
        refreshTask = Task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                await refresh()
            }
        }
    }

    func stopMonitoring() {
        refreshTask?.cancel()
        refreshTask = nil
    }

    func refresh() async {
        peers = await service.getDiscoveredPeers()
        cellInfo = await service.getCellInfo()
        syncStatus = await service.getSyncStatus()
        lastSyncTime = await service.getLastSyncTime()
        activeTransports = await service.getActiveTransports()
        syncEvents = await service.getSyncEvents()
        nodeId = await service.getNodeId()
        nodeName = await service.getNodeName()
    }

    func setNodeName(_ name: String) async {
        await service.setNodeName(name)
        nodeName = name
    }

    var connectedPeerCount: Int { peers.filter(\.isCellMember).count }

    var lastSyncTimeFormatted: String {
        guard let time = lastSyncTime else { return "Never" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: time, relativeTo: Date())
    }
}
