//
//  HiveViewModel.swift
//  HiveTest
//
//  Main view model coordinating HIVE BLE mesh operations
//  Uses CoreBluetooth directly to discover real HIVE nodes
//  Peer management and document sync handled by Rust HiveMesh
//

import Foundation
import Combine
import CoreBluetooth

/// Flush stdout after print to ensure logs appear immediately
func log(_ message: String) {
    print(message)
    fflush(stdout)
}

// Rust hive-btle UniFFI bindings are in HiveBridge/hive_apple_ffi.swift
// Functions: getDefaultMeshId(), parseHiveDeviceName(), matchesMesh(), generateHiveDeviceName()
// HiveMeshWrapper: Centralized peer management, document sync, event handling

// MARK: - HIVE Service UUIDs

/// HIVE BLE Service UUID (canonical 128-bit UUID)
/// Must match: f47ac10b-58cc-4372-a567-0e02b2c3d479
let HIVE_SERVICE_UUID = CBUUID(string: "F47AC10B-58CC-4372-A567-0E02B2C3D479")

/// HIVE Sync Data Characteristic UUID (canonical)
/// Must match: f47a0003-58cc-4372-a567-0e02b2c3d479
let HIVE_DOC_CHAR_UUID = CBUUID(string: "F47A0003-58CC-4372-A567-0E02B2C3D479")

// MARK: - BLE Manager

/// CoreBluetooth manager for HIVE BLE scanning, connections, and advertising
class HiveBLEManager: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate, CBPeripheralManagerDelegate {
    private var centralManager: CBCentralManager!
    private var peripheralManager: CBPeripheralManager!
    private var discoveredPeripherals: [String: CBPeripheral] = [:]
    private var connectedPeripherals: [String: CBPeripheral] = [:]  // Peripherals we connected to as Central
    private var subscribedCentrals: [CBCentral] = []  // Centrals subscribed to our notifications
    private var hiveService: CBMutableService?
    private var syncDataCharacteristic: CBMutableCharacteristic?

    /// Local node ID and device name for advertising
    var localNodeId: UInt32 = 0
    var localDeviceName: String = "HIVE-00000000"

    var onStateChanged: ((CBManagerState) -> Void)?
    var onPeerDiscovered: ((String, String?, Int, Data?, Data?) -> Void)?  // identifier, name, rssi, manufacturerData, serviceData
    var onPeerConnected: ((String) -> Void)?
    var onPeerDisconnected: ((String) -> Void)?
    var onDataReceived: ((String, Data) -> Void)?

    override init() {
        super.init()
        centralManager = CBCentralManager(delegate: self, queue: nil)
        peripheralManager = CBPeripheralManager(delegate: self, queue: nil)
    }

    var state: CBManagerState {
        centralManager.state
    }

    // MARK: - Peripheral (Advertising) Mode

    private func setupGattService() {
        syncDataCharacteristic = CBMutableCharacteristic(
            type: HIVE_DOC_CHAR_UUID,
            properties: [.read, .write, .notify],
            value: nil,
            permissions: [.readable, .writeable]
        )
        hiveService = CBMutableService(type: HIVE_SERVICE_UUID, primary: true)
        hiveService?.characteristics = [syncDataCharacteristic!]
        peripheralManager.add(hiveService!)
    }

    private func startAdvertising() {
        guard peripheralManager.state == .poweredOn else { return }
        let advertisementData: [String: Any] = [
            CBAdvertisementDataLocalNameKey: localDeviceName,
            CBAdvertisementDataServiceUUIDsKey: [HIVE_SERVICE_UUID]
        ]
        peripheralManager.startAdvertising(advertisementData)
    }

    func stopAdvertising() {
        peripheralManager.stopAdvertising()
    }

    // MARK: - CBPeripheralManagerDelegate

    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        if peripheral.state == .poweredOn {
            setupGattService()
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didAdd service: CBService, error: Error?) {
        if error == nil {
            startAdvertising()
        }
    }

    func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager, error: Error?) {
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        if request.characteristic.uuid == HIVE_DOC_CHAR_UUID {
            var nodeId = localNodeId
            let data = Data(bytes: &nodeId, count: 4)
            request.value = data
            peripheral.respond(to: request, withResult: .success)
        } else {
            peripheral.respond(to: request, withResult: .attributeNotFound)
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        for request in requests {
            if let data = request.value {
                onDataReceived?("peripheral", data)
            }
            peripheral.respond(to: request, withResult: .success)
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didSubscribeTo characteristic: CBCharacteristic) {
        if !subscribedCentrals.contains(where: { $0.identifier == central.identifier }) {
            subscribedCentrals.append(central)
            log("[BLE] Central subscribed (total: \(subscribedCentrals.count))")
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom characteristic: CBCharacteristic) {
        subscribedCentrals.removeAll { $0.identifier == central.identifier }
    }

    /// Send data to all connected peers (both as Central and Peripheral)
    func sendData(_ data: Data) {
        var sent = 0

        // Send to subscribed centrals (when we're acting as Peripheral)
        if let characteristic = syncDataCharacteristic, !subscribedCentrals.isEmpty {
            let success = peripheralManager.updateValue(data, for: characteristic, onSubscribedCentrals: nil)
            if success { sent += subscribedCentrals.count }
            log("[BLE] Notify → \(subscribedCentrals.count) centrals (success=\(success))")
        }

        // Send to connected peripherals (when we're acting as Central)
        for (_, peripheral) in connectedPeripherals {
            if let services = peripheral.services,
               let hiveService = services.first(where: { $0.uuid == HIVE_SERVICE_UUID }),
               let chars = hiveService.characteristics,
               let syncChar = chars.first(where: { $0.uuid == HIVE_DOC_CHAR_UUID }) {
                peripheral.writeValue(data, for: syncChar, type: .withResponse)
                sent += 1
                log("[BLE] Write → \(peripheral.name ?? "?") (\(data.count) bytes)")
            }
        }

        if sent == 0 {
            log("[BLE] WARNING: No peers to send to! (peripherals=\(connectedPeripherals.count), centrals=\(subscribedCentrals.count))")
        }
    }

    // MARK: - Central (Scanning) Mode

    func startScanning() {
        guard centralManager.state == .poweredOn else { return }
        centralManager.scanForPeripherals(
            withServices: [HIVE_SERVICE_UUID],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: true]
        )
    }

    func stopScanning() {
        centralManager.stopScan()
    }

    func connect(identifier: String) {
        guard let peripheral = discoveredPeripherals[identifier] else { return }
        centralManager.connect(peripheral, options: nil)
    }

    func disconnect(identifier: String) {
        guard let peripheral = discoveredPeripherals[identifier] else { return }
        centralManager.cancelPeripheralConnection(peripheral)
    }

    // MARK: - CBCentralManagerDelegate

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        onStateChanged?(central.state)
        if central.state == .poweredOn {
            startScanning()
        }
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
                        advertisementData: [String: Any], rssi RSSI: NSNumber) {
        let identifier = peripheral.identifier.uuidString
        let name = peripheral.name ?? advertisementData[CBAdvertisementDataLocalNameKey] as? String
        let rssi = RSSI.intValue

        // Get manufacturer data (contains node ID on some devices)
        let manufacturerData = advertisementData[CBAdvertisementDataManufacturerDataKey] as? Data

        // Get service data (Android HIVE puts node ID here)
        var serviceData: Data? = nil
        if let serviceDataDict = advertisementData[CBAdvertisementDataServiceDataKey] as? [CBUUID: Data] {
            serviceData = serviceDataDict[HIVE_SERVICE_UUID]
            if serviceData == nil {
                serviceData = serviceDataDict[CBUUID(string: "f47ac10b-58cc-4372-a567-0e02b2c3d479")]
            }
        }

        // Store peripheral reference for connection
        discoveredPeripherals[identifier] = peripheral
        onPeerDiscovered?(identifier, name, rssi, manufacturerData, serviceData)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        let identifier = peripheral.identifier.uuidString
        peripheral.delegate = self
        connectedPeripherals[identifier] = peripheral
        peripheral.discoverServices([HIVE_SERVICE_UUID])
        onPeerConnected?(identifier)
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        let identifier = peripheral.identifier.uuidString
        connectedPeripherals.removeValue(forKey: identifier)
        onPeerDisconnected?(identifier)
    }

    var onConnectionFailed: ((String) -> Void)?

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        onConnectionFailed?(peripheral.identifier.uuidString)
    }

    // MARK: - CBPeripheralDelegate

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let services = peripheral.services else { return }
        for service in services {
            peripheral.discoverCharacteristics([HIVE_DOC_CHAR_UUID], for: service)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard let characteristics = service.characteristics else { return }
        for char in characteristics {
            if char.uuid == HIVE_DOC_CHAR_UUID {
                log("[BLE] Found char props=\(char.properties.rawValue) (write=\(char.properties.contains(.write)), writeNoRsp=\(char.properties.contains(.writeWithoutResponse)))")
                peripheral.setNotifyValue(true, for: char)
                peripheral.readValue(for: char)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let data = characteristic.value else { return }
        onDataReceived?(peripheral.identifier.uuidString, data)
    }

    func peripheral(_ peripheral: CBPeripheral, didWriteValueFor characteristic: CBCharacteristic, error: Error?) {
        if let error = error {
            log("[BLE] Write FAILED: \(error.localizedDescription)")
        } else {
            log("[BLE] Write confirmed OK")
        }
    }
}

// MARK: - MeshEventHandler

/// Bridge from Rust MeshEventCallback to Swift @MainActor updates
class MeshEventHandler: MeshEventCallback {
    weak var viewModel: HiveViewModel?

    init(viewModel: HiveViewModel) {
        self.viewModel = viewModel
    }

    func onEvent(event: MeshEvent) {
        // Dispatch to main actor for UI updates
        Task { @MainActor [weak self] in
            self?.viewModel?.handleMeshEvent(event)
        }
    }
}

// MARK: - HiveViewModel

/// Main view model for HIVE BLE mesh operations
/// CoreBluetooth handling remains in Swift, but peer management
/// and document sync are delegated to Rust HiveMeshWrapper
@MainActor
class HiveViewModel: ObservableObject {
    // MARK: - Constants

    /// UserDefaults key for persisted node ID
    private static let nodeIdKey = "hive_node_id"

    /// Mesh ID - identifies which HIVE mesh this node belongs to
    /// Nodes only auto-connect to peers with the same mesh ID
    /// Format: 4-character alphanumeric (e.g., "DEMO", "ALFA", "TEST")
    /// This is provided by the Rust hive-btle crate via UniFFI
    static let MESH_ID: String = getDefaultMeshId()

    /// Get or generate a persistent node ID
    /// Uses last 4 bytes of a generated UUID, similar to MAC-based derivation
    private static func getOrCreateNodeId() -> UInt32 {
        let defaults = UserDefaults.standard

        // Check if we have a saved node ID
        if defaults.object(forKey: nodeIdKey) != nil {
            return UInt32(bitPattern: Int32(truncatingIfNeeded: defaults.integer(forKey: nodeIdKey)))
        }

        // Generate new node ID from UUID (similar to MAC derivation - use last 4 bytes)
        let uuid = UUID()
        let uuidBytes = withUnsafeBytes(of: uuid.uuid) { Array($0) }
        // Use bytes 12-15 (last 4 bytes) like NodeId::from_mac_address uses last 4 of MAC
        let nodeId = (UInt32(uuidBytes[12]) << 24)
                   | (UInt32(uuidBytes[13]) << 16)
                   | (UInt32(uuidBytes[14]) << 8)
                   | UInt32(uuidBytes[15])

        // Persist it
        defaults.set(Int(Int32(bitPattern: nodeId)), forKey: nodeIdKey)
        print("[HiveDemo] Generated new persistent node ID: \(String(format: "%08X", nodeId))")

        return nodeId
    }

    /// Local node ID (persistent across app launches)
    static let NODE_ID: UInt32 = getOrCreateNodeId()

    // MARK: - Published State

    /// Peers in the mesh (derived from HiveMesh)
    @Published var peers: [HivePeer] = []

    /// Current mesh status message
    @Published var statusMessage: String = "Initializing..."

    /// Whether mesh is active
    @Published var isMeshActive: Bool = false

    /// Alert tracking state
    @Published var ackStatus: AckStatus = AckStatus()

    /// Toast message to display temporarily
    @Published var toastMessage: String?

    /// Bluetooth state
    @Published var bluetoothState: LocalBluetoothState = .unknown

    /// Track which node's emergency we've already ACK'd (to suppress re-triggering)
    /// Cleared when user presses RESET
    private var ackedEmergencyNodeId: UInt32?

    /// Local node ID
    let localNodeId: UInt32 = NODE_ID

    // MARK: - Computed Properties

    /// Connected peer count (from HiveMesh)
    var connectedCount: Int {
        Int(hiveMesh?.connectedCount() ?? 0)
    }

    /// Total peer count (from HiveMesh)
    var totalPeerCount: Int {
        Int(hiveMesh?.peerCount() ?? 0)
    }

    /// Display name for local node (from HiveMesh)
    var localDisplayName: String {
        hiveMesh?.deviceName() ?? generateHiveDeviceName(meshId: Self.MESH_ID, nodeId: localNodeId)
    }

    // MARK: - Private Properties

    private var bleManager: HiveBLEManager?
    private var hiveMesh: HiveMeshWrapper?
    private var meshEventHandler: MeshEventHandler?
    private var maintenanceTimer: Timer?

    // MARK: - Initialization

    init() {
        log("[HIVE] Node: \(String(format: "%08X", localNodeId))")
    }

    /// Initialize and start the HIVE mesh
    func startMesh() {
        guard !isMeshActive else { return }

        // Initialize Rust HiveMesh for peer management & document sync
        hiveMesh = HiveMeshWrapper(
            nodeId: localNodeId,
            callsign: "SWIFT",
            meshId: Self.MESH_ID,
            peripheralType: .soldierSensor
        )

        // Set up event observer
        meshEventHandler = MeshEventHandler(viewModel: self)
        hiveMesh?.addObserver(callback: meshEventHandler!)

        // Initialize BLE manager
        bleManager = HiveBLEManager()

        // Configure for advertising (peripheral mode)
        bleManager?.localNodeId = localNodeId
        bleManager?.localDeviceName = hiveMesh?.deviceName() ?? localDisplayName

        bleManager?.onStateChanged = { [weak self] state in
            Task { @MainActor [weak self] in
                self?.handleBLEStateChange(state)
            }
        }

        bleManager?.onPeerDiscovered = { [weak self] identifier, name, rssi, mfgData, svcData in
            Task { @MainActor [weak self] in
                self?.handlePeerDiscovered(identifier: identifier, name: name, rssi: rssi, manufacturerData: mfgData, serviceData: svcData)
            }
        }

        bleManager?.onPeerConnected = { [weak self] identifier in
            Task { @MainActor [weak self] in
                self?.handlePeerConnected(identifier: identifier)
            }
        }

        bleManager?.onPeerDisconnected = { [weak self] identifier in
            Task { @MainActor [weak self] in
                self?.handlePeerDisconnected(identifier: identifier)
            }
        }

        bleManager?.onDataReceived = { [weak self] identifier, data in
            Task { @MainActor [weak self] in
                self?.handleDataReceived(identifier: identifier, data: data)
            }
        }

        bleManager?.onConnectionFailed = { [weak self] identifier in
            Task { @MainActor [weak self] in
                self?.handleConnectionFailed(identifier: identifier)
            }
        }

        isMeshActive = true
        statusMessage = "Scanning for HIVE nodes..."

        // Periodic maintenance timer (peer cleanup, sync) - 1 second for responsive connection tracking
        maintenanceTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.performMaintenance()
            }
        }
    }

    /// Shutdown the mesh
    func shutdown() {
        print("[HiveDemo] Shutting down HIVE mesh...")

        maintenanceTimer?.invalidate()
        maintenanceTimer = nil
        bleManager?.stopScanning()
        bleManager?.stopAdvertising()
        bleManager = nil
        meshEventHandler = nil
        hiveMesh = nil
        isMeshActive = false
        peers.removeAll()
        ackStatus.reset()
        statusMessage = "Mesh stopped"
    }

    /// Connect to a peer
    func connect(to peer: HivePeer) {
        bleManager?.connect(identifier: peer.identifier)
        showToast("Connecting to \(peer.displayName)...")
    }

    /// Disconnect from a peer
    func disconnect(from peer: HivePeer) {
        bleManager?.disconnect(identifier: peer.identifier)
    }

    // MARK: - BLE Event Handlers

    private func handleBLEStateChange(_ state: CBManagerState) {
        switch state {
        case .poweredOn:
            bluetoothState = .poweredOn
            statusMessage = "Mesh active - \(localDisplayName)"
        case .poweredOff:
            bluetoothState = .poweredOff
            statusMessage = "Bluetooth is off"
        case .unauthorized:
            bluetoothState = .unauthorized
            statusMessage = "Bluetooth not authorized"
        case .unsupported:
            bluetoothState = .unsupported
            statusMessage = "Bluetooth not supported"
        default:
            bluetoothState = .unknown
        }
    }

    /// Track which peers we've already logged discovery for
    private static var loggedDiscoveries: Set<UInt32> = []

    private func handlePeerDiscovered(identifier: String, name: String?, rssi: Int, manufacturerData: Data?, serviceData: Data?) {
        guard let mesh = hiveMesh else { return }

        // Parse mesh ID from name
        var meshId: String? = nil
        if let name = name, let parsed = parseHiveDeviceName(name: name) {
            meshId = parsed.meshId
        }

        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)

        // Delegate to HiveMesh - it handles peer tracking, filtering, and deduplication
        if let meshPeer = mesh.onBleDiscovered(
            identifier: identifier,
            name: name,
            rssi: Int8(clamping: rssi),
            meshId: meshId,
            nowMs: nowMs
        ) {
            // Only log first discovery of each peer
            if !Self.loggedDiscoveries.contains(meshPeer.nodeId) {
                Self.loggedDiscoveries.insert(meshPeer.nodeId)
                log("[HIVE] Discovered: \(String(format: "%08X", meshPeer.nodeId))")
            }

            // Update local peers list from HiveMesh
            syncPeersFromMesh()

            // Auto-connect if it matches our mesh and isn't ourselves
            if meshPeer.nodeId != localNodeId && mesh.matchesMesh(deviceMeshId: meshId) {
                bleManager?.connect(identifier: identifier)
            }
        }
    }

    private func handlePeerConnected(identifier: String) {
        guard let mesh = hiveMesh else { return }
        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)

        if let nodeId = mesh.onBleConnected(identifier: identifier, nowMs: nowMs) {
            log("[HIVE] Connected: \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()
        }
    }

    private func handlePeerDisconnected(identifier: String) {
        guard let mesh = hiveMesh else { return }

        if let nodeId = mesh.onBleDisconnected(identifier: identifier, reason: .linkLoss) {
            log("[HIVE] Disconnected: \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()
        }
    }

    private func handleConnectionFailed(identifier: String) {
        guard let mesh = hiveMesh else { return }
        _ = mesh.onBleDisconnected(identifier: identifier, reason: .connectionFailed)
        syncPeersFromMesh()
    }

    private func handleDataReceived(identifier: String, data: Data) {
        guard let mesh = hiveMesh else { return }
        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)

        // Delegate document parsing and merging to HiveMesh
        if let result = mesh.onBleDataReceived(identifier: identifier, data: data, nowMs: nowMs) {
            syncPeersFromMesh()

            // Handle events
            if result.isEmergency {
                handleEmergencyReceivedFromNode(result.sourceNode)
            } else if result.isAck {
                handleAckReceivedFromNode(result.sourceNode)
            }
        }
    }

    /// Handle emergency received (called from mesh event or data parsing)
    private func handleEmergencyReceivedFromNode(_ nodeId: UInt32) {
        // Don't re-trigger if already in alert mode for the same emergency
        if ackStatus.isActive && ackStatus.emergencySourceNodeId == nodeId {
            return
        }

        // Don't re-trigger if we already ACK'd this node's emergency
        // User must press RESET to clear this and allow new emergencies from same node
        if ackedEmergencyNodeId == nodeId {
            log("[HiveDemo] Suppressing emergency re-trigger (already ACK'd node \(String(format: "%08X", nodeId)))")
            return
        }

        log("[HiveDemo] EMERGENCY from \(String(format: "%08X", nodeId))")

        // Initialize ACK tracking
        ackStatus.pendingAcks.removeAll()
        for peer in peers {
            ackStatus.pendingAcks[peer.nodeId] = false
        }
        ackStatus.pendingAcks[localNodeId] = false  // We haven't acked yet
        ackStatus.pendingAcks[nodeId] = true  // Source has implicitly acked
        ackStatus.emergencySourceNodeId = nodeId

        showToast("🚨 EMERGENCY from \(String(format: "HIVE-%08X", nodeId))!")
        statusMessage = "⚠️ EMERGENCY - TAP ACK"
        triggerVibration()
    }

    /// Handle ACK received (called from mesh event or data parsing)
    private func handleAckReceivedFromNode(_ nodeId: UInt32) {
        log("[HiveDemo] ACK from \(String(format: "%08X", nodeId))")
        ackStatus.pendingAcks[nodeId] = true
        showToast("✓ ACK from \(String(format: "HIVE-%08X", nodeId))")
        checkAllAcked()
    }

    /// Periodic maintenance - delegates to HiveMesh.tick()
    private func performMaintenance() {
        guard let mesh = hiveMesh else { return }
        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)

        // tick() handles peer cleanup
        _ = mesh.tick(nowMs: nowMs)

        // Always send current document as heartbeat (keeps connection alive for peer tracking)
        let document = mesh.buildDocument()
        bleManager?.sendData(Data(document))

        // Refresh peers from mesh
        syncPeersFromMesh()
    }

    /// Sync local peers array from HiveMesh state
    private func syncPeersFromMesh() {
        guard let mesh = hiveMesh else { return }

        let meshPeers = mesh.getPeers()
        peers = meshPeers.map { mp in
            HivePeer(
                identifier: mp.identifier,
                nodeId: mp.nodeId,
                meshId: mp.meshId,
                advertisedName: mp.name,
                isConnected: mp.isConnected,
                rssi: mp.rssi,
                lastSeen: Date(timeIntervalSince1970: Double(mp.lastSeenMs) / 1000.0)
            )
        }
        // Sort by RSSI (strongest first)
        peers.sort { $0.rssi > $1.rssi }
    }

    /// Handle mesh events from Rust HiveMesh observer
    func handleMeshEvent(_ event: MeshEvent) {
        switch event {
        case .peerDiscovered(_):
            syncPeersFromMesh()
        case .peerConnected(_):
            syncPeersFromMesh()
        case .peerDisconnected(_, _):
            syncPeersFromMesh()
        case .peerLost(_):
            syncPeersFromMesh()
        case .emergencyReceived(let fromNode):
            handleEmergencyReceivedFromNode(fromNode)
        case .ackReceived(let fromNode):
            handleAckReceivedFromNode(fromNode)
        case .documentSynced(_, _):
            break
        case .meshStateChanged(_, _):
            syncPeersFromMesh()
        }
    }

    // MARK: - User Actions (delegate to HiveMesh)

    /// Send an emergency alert to all peers
    func sendEmergency() {
        guard isMeshActive, let mesh = hiveMesh else {
            showToast("Mesh not active")
            return
        }

        print("[HiveDemo] >>> SENDING EMERGENCY")

        // Initialize ACK tracking
        ackStatus.pendingAcks.removeAll()
        for peer in peers {
            ackStatus.pendingAcks[peer.nodeId] = false
        }
        ackStatus.pendingAcks[localNodeId] = true  // We sent it, so we're acked
        ackStatus.emergencySourceNodeId = localNodeId

        // Build emergency document via HiveMesh and broadcast
        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let document = mesh.sendEmergency(timestamp: timestamp)
        bleManager?.sendData(Data(document))

        showToast("🚨 EMERGENCY SENT!")
        statusMessage = "⚠️ EMERGENCY - TAP ACK"
    }

    /// Send an ACK
    func sendAck() {
        guard isMeshActive, let mesh = hiveMesh else {
            showToast("Mesh not active")
            return
        }

        log("[HiveDemo] >>> SENDING ACK")
        log("[HiveDemo] Peers: \(peers.count), connected: \(connectedCount)")
        for peer in peers {
            log("[HiveDemo]   Peer \(String(format: "%08X", peer.nodeId)): connected=\(peer.isConnected), id=\(peer.identifier)")
        }

        // Record which node's emergency we ACK'd (to suppress re-triggering from CRDT sync)
        ackedEmergencyNodeId = ackStatus.emergencySourceNodeId

        // Build ACK document via HiveMesh and broadcast
        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let document = mesh.sendAck(timestamp: timestamp)
        log("[HiveDemo] ACK document: \(document.count) bytes")
        bleManager?.sendData(Data(document))

        // Clear local alert state and reset ACK tracking
        hiveMesh?.clearEvent()
        ackStatus.reset()
        statusMessage = "Mesh active - \(localDisplayName)"

        showToast("✓ ACK sent")
    }

    /// Reset the alert state
    func resetAlert() {
        print("[HiveDemo] >>> RESETTING ALERT")

        hiveMesh?.clearEvent()
        ackStatus.reset()
        ackedEmergencyNodeId = nil  // Clear to allow new emergencies from same node
        statusMessage = "Mesh active - \(localDisplayName)"
        showToast("Alert reset")
    }

    // MARK: - Private Helpers

    private func checkAllAcked() {
        if ackStatus.allAcked {
            ackStatus.reset()
            statusMessage = "✓ All peers acknowledged"
        }
    }

    private func showToast(_ message: String) {
        toastMessage = message

        Task {
            try? await Task.sleep(nanoseconds: 2_000_000_000)
            if toastMessage == message {
                toastMessage = nil
            }
        }
    }

    private func triggerVibration() {
        #if os(iOS)
        let generator = UINotificationFeedbackGenerator()
        generator.notificationOccurred(.error)
        #endif
    }
}

// MARK: - Bluetooth State (Local)

/// Local Bluetooth state enum (distinct from UniFFI BluetoothState)
enum LocalBluetoothState: String {
    case unknown = "Unknown"
    case resetting = "Resetting"
    case unsupported = "Unsupported"
    case unauthorized = "Unauthorized"
    case poweredOff = "Powered Off"
    case poweredOn = "Powered On"

    var isReady: Bool {
        self == .poweredOn
    }
}

#if os(iOS)
import UIKit
#endif
