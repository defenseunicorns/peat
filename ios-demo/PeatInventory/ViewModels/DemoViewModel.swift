import Foundation

@Observable
final class DemoViewModel {
    var isDemoMode: Bool = false
    var selectedScenario: DemoScenario?
    var scenarioRunning: Bool = false
    var scenarioLog: [String] = []
    var selectedNodeName: String = DemoDataService.demoNodeNames[0]

    private let service: PeatServiceProtocol
    private let inventoryVM: InventoryViewModel
    private let meshVM: MeshViewModel

    init(service: PeatServiceProtocol, inventoryVM: InventoryViewModel, meshVM: MeshViewModel) {
        self.service = service
        self.inventoryVM = inventoryVM
        self.meshVM = meshVM
    }

    func enableDemoMode() async {
        isDemoMode = true
        await service.setNodeName(selectedNodeName)
        await inventoryVM.loadDemoData()
        await meshVM.refresh()
        scenarioLog.append("[\(timestamp)] Demo mode enabled as \(selectedNodeName)")
    }

    func updateNodeName(_ name: String) async {
        selectedNodeName = name
        await service.setNodeName(name)
    }

    func disableDemoMode() async {
        isDemoMode = false
        scenarioRunning = false
        selectedScenario = nil
        scenarioLog.removeAll()
    }

    func runScenario(_ scenario: DemoScenario) async {
        selectedScenario = scenario
        scenarioRunning = true
        scenarioLog.append("[\(timestamp)] Starting scenario: \(scenario.title)")

        switch scenario {
        case .realTimeSync:
            await runRealTimeSyncScenario()
        case .splitMerge:
            await runSplitMergeScenario()
        case .transportFailover:
            await runTransportFailoverScenario()
        case .commandPostRollUp:
            await runCommandPostRollUpScenario()
        }

        scenarioRunning = false
        scenarioLog.append("[\(timestamp)] Scenario complete")
    }

    private func runRealTimeSyncScenario() async {
        scenarioLog.append("[\(timestamp)] Add an item on this device — watch it appear on the peer")
        scenarioLog.append("[\(timestamp)] Waiting for peer connection...")
        // The actual sync happens via MultipeerConnectivity in PeatServiceMock
        // This scenario is primarily manual — the demo operator adds an item
        try? await Task.sleep(for: .seconds(2))
        scenarioLog.append("[\(timestamp)] Ready — add an item to demonstrate real-time sync")
    }

    private func runSplitMergeScenario() async {
        scenarioLog.append("[\(timestamp)] Step 1: Both devices go offline")
        scenarioLog.append("[\(timestamp)] Step 2: Each device edits different items")
        scenarioLog.append("[\(timestamp)] Step 3: Reconnect — CRDT merge resolves automatically")
        try? await Task.sleep(for: .seconds(2))
        scenarioLog.append("[\(timestamp)] Ready — disconnect Wi-Fi, make edits, then reconnect")
    }

    private func runTransportFailoverScenario() async {
        scenarioLog.append("[\(timestamp)] Step 1: Verify sync over Wi-Fi")
        scenarioLog.append("[\(timestamp)] Step 2: Disable Wi-Fi on both devices")
        scenarioLog.append("[\(timestamp)] Step 3: BLE transport activates automatically")
        try? await Task.sleep(for: .seconds(2))
        scenarioLog.append("[\(timestamp)] Ready — turn off Wi-Fi to trigger BLE failover")
    }

    private func runCommandPostRollUpScenario() async {
        scenarioLog.append("[\(timestamp)] Step 1: Two field devices sync independently")
        scenarioLog.append("[\(timestamp)] Step 2: Command post device joins the cell")
        scenarioLog.append("[\(timestamp)] Step 3: Aggregation view shows combined inventory")
        try? await Task.sleep(for: .seconds(2))
        scenarioLog.append("[\(timestamp)] Ready — connect a third device to see aggregated data")
    }

    private var timestamp: String {
        let fmt = DateFormatter()
        fmt.dateFormat = "HH:mm:ss"
        return fmt.string(from: Date())
    }
}

enum DemoScenario: String, CaseIterable, Identifiable {
    case realTimeSync = "Real-Time Sync"
    case splitMerge = "Split-Merge"
    case transportFailover = "Transport Failover"
    case commandPostRollUp = "Command Post Roll-Up"

    var id: String { rawValue }

    var title: String { rawValue }

    var description: String {
        switch self {
        case .realTimeSync: "Add an item, watch it appear on peer device"
        case .splitMerge: "Both devices edit offline, reconnect to merge"
        case .transportFailover: "Sync via Wi-Fi, kill Wi-Fi, BLE takes over"
        case .commandPostRollUp: "Third device aggregates from two field devices"
        }
    }

    var sfSymbol: String {
        switch self {
        case .realTimeSync: "arrow.triangle.2.circlepath"
        case .splitMerge: "arrow.triangle.branch"
        case .transportFailover: "arrow.triangle.swap"
        case .commandPostRollUp: "building.2.fill"
        }
    }
}
