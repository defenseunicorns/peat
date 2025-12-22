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
        // Create the sync data characteristic (read/write/notify)
        syncDataCharacteristic = CBMutableCharacteristic(
            type: HIVE_DOC_CHAR_UUID,
            properties: [.read, .write, .notify],
            value: nil,
            permissions: [.readable, .writeable]
        )

        // Create the HIVE service
        hiveService = CBMutableService(type: HIVE_SERVICE_UUID, primary: true)
        hiveService?.characteristics = [syncDataCharacteristic!]

        // Add service to peripheral manager
        peripheralManager.add(hiveService!)
        print("[BLE Peripheral] Added HIVE service")
    }

    private func startAdvertising() {
        guard peripheralManager.state == .poweredOn else {
            print("[BLE Peripheral] Cannot advertise - not powered on")
            return
        }

        // Advertise with local name and service UUID
        let advertisementData: [String: Any] = [
            CBAdvertisementDataLocalNameKey: localDeviceName,
            CBAdvertisementDataServiceUUIDsKey: [HIVE_SERVICE_UUID]
        ]

        peripheralManager.startAdvertising(advertisementData)
        print("[BLE Peripheral] Started advertising as '\(localDeviceName)'")
    }

    func stopAdvertising() {
        peripheralManager.stopAdvertising()
        print("[BLE Peripheral] Stopped advertising")
    }

    // MARK: - CBPeripheralManagerDelegate

    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        print("[BLE Peripheral] State changed: \(peripheral.state.rawValue)")

        if peripheral.state == .poweredOn {
            setupGattService()
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didAdd service: CBService, error: Error?) {
        if let error = error {
            print("[BLE Peripheral] Failed to add service: \(error.localizedDescription)")
        } else {
            print("[BLE Peripheral] Service added, starting advertising...")
            startAdvertising()
        }
    }

    func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager, error: Error?) {
        if let error = error {
            print("[BLE Peripheral] Failed to start advertising: \(error.localizedDescription)")
        } else {
            print("[BLE Peripheral] Advertising started successfully")
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        log("[BLE Peripheral] Read request for \(request.characteristic.uuid)")

        if request.characteristic.uuid == HIVE_DOC_CHAR_UUID {
            // Return node ID as 4 bytes
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
            let dataSize = request.value?.count ?? 0
            log("[BLE Peripheral] Write request: \(dataSize) bytes")

            if let data = request.value {
                log("[BLE Peripheral] Data: \(data.map { String(format: "%02X", $0) }.joined(separator: " "))")
                // Notify the app of received data
                if onDataReceived != nil {
                    onDataReceived?("peripheral", data)
                } else {
                    log("[BLE Peripheral] WARNING: onDataReceived callback not set!")
                }
            }

            peripheral.respond(to: request, withResult: .success)
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didSubscribeTo characteristic: CBCharacteristic) {
        log("[BLE Peripheral] Central \(central.identifier) subscribed to \(characteristic.uuid)")
        if !subscribedCentrals.contains(where: { $0.identifier == central.identifier }) {
            subscribedCentrals.append(central)
            log("[BLE Peripheral] Now have \(subscribedCentrals.count) subscribed centrals")
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom characteristic: CBCharacteristic) {
        log("[BLE Peripheral] Central unsubscribed from \(characteristic.uuid)")
        subscribedCentrals.removeAll { $0.identifier == central.identifier }
    }

    /// Send data to all connected peers (both as Central and Peripheral)
    func sendData(_ data: Data) {
        print("[BLE] Sending \(data.count) bytes to peers")
        print("[BLE] connectedPeripherals=\(connectedPeripherals.count), subscribedCentrals=\(subscribedCentrals.count)")

        // Send to subscribed centrals (when we're acting as Peripheral)
        if let characteristic = syncDataCharacteristic, !subscribedCentrals.isEmpty {
            let success = peripheralManager.updateValue(data, for: characteristic, onSubscribedCentrals: nil)
            print("[BLE Peripheral] Sent notification to \(subscribedCentrals.count) centrals, success=\(success)")
        }

        // Send to connected peripherals (when we're acting as Central)
        for (identifier, peripheral) in connectedPeripherals {
            print("[BLE Central] Checking peripheral \(identifier): services=\(peripheral.services?.count ?? 0)")
            if let services = peripheral.services {
                for svc in services {
                    print("[BLE Central]   Service: \(svc.uuid), chars=\(svc.characteristics?.count ?? 0)")
                }
            }
            if let services = peripheral.services,
               let hiveService = services.first(where: { $0.uuid == HIVE_SERVICE_UUID }),
               let chars = hiveService.characteristics,
               let syncChar = chars.first(where: { $0.uuid == HIVE_DOC_CHAR_UUID }) {
                peripheral.writeValue(data, for: syncChar, type: .withResponse)
                print("[BLE Central] Wrote \(data.count) bytes to peripheral \(identifier)")
            } else {
                print("[BLE Central] No HIVE service/char found on \(identifier)")
            }
        }
    }

    // MARK: - Central (Scanning) Mode

    func startScanning() {
        guard centralManager.state == .poweredOn else {
            print("[BLE] Cannot scan - Bluetooth not powered on")
            return
        }

        print("[BLE] Starting scan for HIVE service \(HIVE_SERVICE_UUID)")
        centralManager.scanForPeripherals(
            withServices: [HIVE_SERVICE_UUID],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: true]
        )
    }

    func stopScanning() {
        centralManager.stopScan()
        print("[BLE] Stopped scanning")
    }

    func connect(identifier: String) {
        guard let peripheral = discoveredPeripherals[identifier] else {
            print("[BLE] Peripheral not found: \(identifier)")
            return
        }
        print("[BLE] Connecting to \(peripheral.name ?? identifier)")
        centralManager.connect(peripheral, options: nil)
    }

    func disconnect(identifier: String) {
        guard let peripheral = discoveredPeripherals[identifier] else { return }
        centralManager.cancelPeripheralConnection(peripheral)
    }

    // MARK: - CBCentralManagerDelegate

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        print("[BLE] State changed: \(central.state.rawValue)")
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
            // Log what UUIDs are in the service data dict
            for (uuid, data) in serviceDataDict {
                print("[BLE] ServiceData UUID: \(uuid) len=\(data.count) hex=\(data.map { String(format: "%02X", $0) }.joined())")
            }
            serviceData = serviceDataDict[HIVE_SERVICE_UUID]
            // Also try lowercase UUID
            if serviceData == nil {
                serviceData = serviceDataDict[CBUUID(string: "f47ac10b-58cc-4372-a567-0e02b2c3d479")]
            }
        }

        // Store peripheral reference for connection
        discoveredPeripherals[identifier] = peripheral

        print("[BLE] Discovered: \(name ?? "Unknown") RSSI=\(rssi) svcData=\(serviceData?.count ?? 0)")
        onPeerDiscovered?(identifier, name, rssi, manufacturerData, serviceData)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        let identifier = peripheral.identifier.uuidString
        log("[BLE] Connected to \(peripheral.name ?? identifier)")
        peripheral.delegate = self
        connectedPeripherals[identifier] = peripheral
        // Discover ALL services to see what Android exposes
        peripheral.discoverServices(nil)
        onPeerConnected?(identifier)
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        let identifier = peripheral.identifier.uuidString
        print("[BLE] Disconnected from \(peripheral.name ?? identifier)")
        connectedPeripherals.removeValue(forKey: identifier)
        onPeerDisconnected?(identifier)
    }

    var onConnectionFailed: ((String) -> Void)?

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        print("[BLE] Failed to connect: \(error?.localizedDescription ?? "unknown")")
        onConnectionFailed?(peripheral.identifier.uuidString)
    }

    // MARK: - CBPeripheralDelegate

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        if let error = error {
            log("[BLE] Service discovery error: \(error)")
            return
        }
        guard let services = peripheral.services else {
            log("[BLE] No services found on \(peripheral.name ?? "unknown")")
            return
        }
        log("[BLE] Found \(services.count) services on \(peripheral.name ?? "unknown")")
        for service in services {
            log("[BLE] Service: \(service.uuid)")
            // Discover ALL characteristics to see what's available
            peripheral.discoverCharacteristics(nil, for: service)
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard let characteristics = service.characteristics else { return }
        log("[BLE] Service \(service.uuid) has \(characteristics.count) characteristics:")
        for char in characteristics {
            log("[BLE]   Char: \(char.uuid) props=\(char.properties.rawValue)")
            if char.uuid == HIVE_DOC_CHAR_UUID {
                log("[BLE] Found HIVE doc characteristic!")
                // Subscribe to notifications
                peripheral.setNotifyValue(true, for: char)
                // Read current value
                peripheral.readValue(for: char)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let data = characteristic.value else { return }
        print("[BLE] Received \(data.count) bytes from \(peripheral.name ?? "unknown")")
        onDataReceived?(peripheral.identifier.uuidString, data)
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
        print("[HiveDemo] Initializing with node ID: \(String(format: "%08X", localNodeId))")
    }

    /// Initialize and start the HIVE mesh
    func startMesh() {
        guard !isMeshActive else { return }

        print("[HiveDemo] Starting HIVE mesh with CoreBluetooth + HiveMeshWrapper...")

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

        // Periodic maintenance timer (peer cleanup, sync)
        maintenanceTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
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
            log("[HiveDemo] HiveMesh discovered peer: \(meshPeer.name ?? String(format: "HIVE-%08X", meshPeer.nodeId))")

            // Update local peers list from HiveMesh
            syncPeersFromMesh()

            // Auto-connect if it matches our mesh and isn't ourselves
            if meshPeer.nodeId != localNodeId && mesh.matchesMesh(deviceMeshId: meshId) {
                log("[HiveDemo] >>> Auto-connecting to \(String(format: "HIVE-%08X", meshPeer.nodeId))...")
                bleManager?.connect(identifier: identifier)
            }
        }
    }

    private func handlePeerConnected(identifier: String) {
        guard let mesh = hiveMesh else { return }
        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)

        if let nodeId = mesh.onBleConnected(identifier: identifier, nowMs: nowMs) {
            log("[HiveDemo] HiveMesh connected: \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()
            showToast("Connected to \(String(format: "HIVE-%08X", nodeId))")
        }
    }

    private func handlePeerDisconnected(identifier: String) {
        guard let mesh = hiveMesh else { return }

        if let nodeId = mesh.onBleDisconnected(identifier: identifier, reason: .linkLoss) {
            log("[HiveDemo] HiveMesh disconnected: \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()
            showToast("Disconnected from \(String(format: "HIVE-%08X", nodeId))")
        }
    }

    private func handleConnectionFailed(identifier: String) {
        guard let mesh = hiveMesh else { return }

        if let nodeId = mesh.onBleDisconnected(identifier: identifier, reason: .connectionFailed) {
            log("[HiveDemo] Connection failed: \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()
        }
    }

    private func handleDataReceived(identifier: String, data: Data) {
        guard let mesh = hiveMesh else { return }
        let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)

        log("[HiveDemo] Received \(data.count) bytes from \(identifier)")

        // Delegate document parsing and merging to HiveMesh
        if let result = mesh.onBleDataReceived(identifier: identifier, data: data, nowMs: nowMs) {
            log("[HiveDemo] Data from \(String(format: "HIVE-%08X", result.sourceNode)): emergency=\(result.isEmergency), ack=\(result.isAck), count=\(result.totalCount)")

            // Sync peers list from mesh (may have added incoming connection)
            syncPeersFromMesh()

            // Handle events - HiveMesh notifies via observer, but we also check here for UI updates
            if result.isEmergency {
                handleEmergencyReceivedFromNode(result.sourceNode)
            } else if result.isAck {
                handleAckReceivedFromNode(result.sourceNode)
            }
        }
    }

    /// Handle emergency received (called from mesh event or data parsing)
    private func handleEmergencyReceivedFromNode(_ nodeId: UInt32) {
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

        // tick() handles peer cleanup and returns sync data if needed
        if let syncData = mesh.tick(nowMs: nowMs) {
            log("[HiveDemo] Maintenance: broadcasting sync document (\(syncData.count) bytes)")
            bleManager?.sendData(Data(syncData))
        }

        // Refresh peers from mesh
        syncPeersFromMesh()

        log("[HiveDemo] Heartbeat: peers=\(mesh.peerCount()), connected=\(mesh.connectedCount()), BLE=\(bleManager?.state.rawValue ?? -1)")
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
        case .peerDiscovered(let peer):
            log("[HiveDemo] Event: PeerDiscovered \(peer.nodeId)")
            syncPeersFromMesh()

        case .peerConnected(let nodeId):
            log("[HiveDemo] Event: PeerConnected \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()

        case .peerDisconnected(let nodeId, _):
            log("[HiveDemo] Event: PeerDisconnected \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()

        case .peerLost(let nodeId):
            log("[HiveDemo] Event: PeerLost \(String(format: "%08X", nodeId))")
            syncPeersFromMesh()

        case .emergencyReceived(let fromNode):
            handleEmergencyReceivedFromNode(fromNode)

        case .ackReceived(let fromNode):
            handleAckReceivedFromNode(fromNode)

        case .documentSynced(let fromNode, let totalCount):
            log("[HiveDemo] Event: DocumentSynced from \(String(format: "%08X", fromNode)), count=\(totalCount)")

        case .meshStateChanged(let peerCount, let connectedCount):
            log("[HiveDemo] Event: MeshStateChanged peers=\(peerCount), connected=\(connectedCount)")
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

        print("[HiveDemo] >>> SENDING ACK")

        // Build ACK document via HiveMesh and broadcast
        let timestamp = UInt64(Date().timeIntervalSince1970 * 1000)
        let document = mesh.sendAck(timestamp: timestamp)
        bleManager?.sendData(Data(document))

        ackStatus.pendingAcks[localNodeId] = true
        showToast("✓ ACK sent")

        checkAllAcked()
    }

    /// Reset the alert state
    func resetAlert() {
        print("[HiveDemo] >>> RESETTING ALERT")

        hiveMesh?.clearEvent()
        ackStatus.reset()
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
