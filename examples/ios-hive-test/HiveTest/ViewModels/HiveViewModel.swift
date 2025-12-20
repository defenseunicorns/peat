//
//  HiveViewModel.swift
//  HiveTest
//
//  Main view model coordinating HIVE BLE mesh operations
//  Uses CoreBluetooth directly to discover real HIVE nodes
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

// MARK: - HiveViewModel

/// Main view model for HIVE BLE mesh operations
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

    /// Peers in the mesh (discovered and/or connected)
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

    /// Connected peer count
    var connectedCount: Int {
        peers.filter { $0.isConnected }.count
    }

    /// Total peer count
    var totalPeerCount: Int {
        peers.count
    }

    /// Display name for local node (includes mesh ID)
    /// Uses Rust hive-btle MeshConfig::device_name() via UniFFI
    var localDisplayName: String {
        generateHiveDeviceName(meshId: Self.MESH_ID, nodeId: localNodeId)
    }

    // MARK: - Private Properties

    private var bleManager: HiveBLEManager?
    private var peerCleanupTimer: Timer?

    // MARK: - Initialization

    init() {
        print("[HiveDemo] Initializing with node ID: \(String(format: "%08X", localNodeId))")
    }

    /// Initialize and start the HIVE mesh
    func startMesh() {
        guard !isMeshActive else { return }

        print("[HiveDemo] Starting HIVE mesh with CoreBluetooth...")

        // Initialize BLE manager
        bleManager = HiveBLEManager()

        // Configure for advertising (peripheral mode)
        bleManager?.localNodeId = localNodeId
        bleManager?.localDeviceName = localDisplayName

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

        // Cleanup stale peers periodically and log scan status
        peerCleanupTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.cleanupStalePeers()
                print("[HiveDemo] Heartbeat: peers=\(self?.peers.count ?? 0), BLE state=\(self?.bleManager?.state.rawValue ?? -1)")
            }
        }
    }

    /// Shutdown the mesh
    func shutdown() {
        print("[HiveDemo] Shutting down HIVE mesh...")

        bleManager?.stopScanning()
        bleManager?.stopAdvertising()
        bleManager = nil
        peerCleanupTimer?.invalidate()
        peerCleanupTimer = nil
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
        // Parse mesh ID and node ID from name using Rust hive-btle via UniFFI
        var nodeId: UInt32 = 0
        var meshId: String? = nil

        if let name = name, let parsed = parseHiveDeviceName(name: name) {
            // Successfully parsed using Rust MeshConfig::parse_device_name()
            meshId = parsed.meshId
            nodeId = parsed.nodeId
        }

        // If no node ID from name, try service data (Android HIVE uses this)
        if nodeId == 0, let svcData = serviceData, svcData.count >= 4 {
            // Service data contains 4-byte node ID directly
            nodeId = svcData.withUnsafeBytes { $0.load(as: UInt32.self) }
            meshId = HiveViewModel.MESH_ID  // Assume same mesh since it's advertising HIVE service
            print("[HiveDemo] Got nodeId from service data: \(String(format: "%08X", nodeId))")
        }

        // If no node ID from service data, try manufacturer data
        if nodeId == 0, let mfgData = manufacturerData, mfgData.count >= 4 {
            // Skip 2-byte company ID, read 4-byte node ID
            if mfgData.count >= 6 {
                nodeId = mfgData.subdata(in: 2..<6).withUnsafeBytes { $0.load(as: UInt32.self) }
            }
        }

        // If we couldn't get a node ID, use a placeholder based on identifier hash
        // The real node ID will be updated when we receive documents
        if nodeId == 0 {
            // Generate temporary node ID from identifier hash
            let hash = identifier.hashValue
            nodeId = UInt32(truncatingIfNeeded: abs(hash))
            meshId = HiveViewModel.MESH_ID  // Assume same mesh since it's advertising HIVE service
            print("[HiveDemo] Using temp nodeId \(String(format: "%08X", nodeId)) for \(name ?? "Unknown") - will update from document")
        }

        // Check if this is a new peer
        let isNewPeer = !peers.contains(where: { $0.identifier == identifier })

        // Update or add peer
        if let index = peers.firstIndex(where: { $0.identifier == identifier }) {
            peers[index].rssi = Int8(clamping: rssi)
            peers[index].lastSeen = Date()
        } else {
            let peer = HivePeer(
                identifier: identifier,
                nodeId: nodeId,
                meshId: meshId,
                advertisedName: name,
                isConnected: false,
                rssi: Int8(clamping: rssi),
                lastSeen: Date()
            )
            peers.append(peer)
            print("[HiveDemo] New HIVE peer: \(peer.displayName) meshId=\(meshId ?? "none") RSSI=\(rssi)")
        }

        // Sort by RSSI (strongest first)
        peers.sort { $0.rssi > $1.rssi }

        print("[HiveDemo] Peers list now has \(peers.count) items: \(peers.map { $0.displayName })")

        // Auto-connect to new HIVE peers in SAME MESH only
        // Don't connect to ourselves!
        log("[HiveDemo] Auto-connect check: isNewPeer=\(isNewPeer), nodeId=\(String(format: "%08X", nodeId)), localNodeId=\(String(format: "%08X", localNodeId))")
        if isNewPeer && nodeId != localNodeId {
            // Check mesh ID match using Rust matchesMesh() via UniFFI
            // Returns true if same mesh or if legacy format (nil mesh ID)
            let sameMesh = matchesMesh(ourMeshId: Self.MESH_ID, deviceMeshId: meshId)
            log("[HiveDemo] Mesh check: ourMesh=\(Self.MESH_ID), deviceMesh=\(meshId ?? "nil"), sameMesh=\(sameMesh)")
            if sameMesh {
                log("[HiveDemo] >>> Auto-connecting to \(String(format: "HIVE-%08X", nodeId)) (mesh: \(meshId ?? "any"))...")
                bleManager?.connect(identifier: identifier)
            } else {
                log("[HiveDemo] Skipping peer \(String(format: "HIVE-%08X", nodeId)) - different mesh (\(meshId ?? "?") != \(Self.MESH_ID))")
            }
        } else if nodeId == localNodeId {
            print("[HiveDemo] Skipping self-connection to \(String(format: "HIVE-%08X", nodeId))")
            // Remove ourselves from the peer list
            peers.removeAll { $0.nodeId == localNodeId }
        } else if !isNewPeer {
            print("[HiveDemo] Peer already known, not reconnecting")
        }
    }

    private func handlePeerConnected(identifier: String) {
        if let index = peers.firstIndex(where: { $0.identifier == identifier }) {
            peers[index].isConnected = true
            showToast("Connected to \(peers[index].displayName)")
        }
    }

    private func handlePeerDisconnected(identifier: String) {
        if let index = peers.firstIndex(where: { $0.identifier == identifier }) {
            let peerName = peers[index].displayName
            // Keep peer in list, just mark as disconnected
            peers[index].isConnected = false
            showToast("Disconnected from \(peerName)")
            print("[HiveDemo] Peer \(peerName) disconnected - keeping in list")
        }
    }

    private func handleConnectionFailed(identifier: String) {
        if let index = peers.firstIndex(where: { $0.identifier == identifier }) {
            let peerName = peers[index].displayName
            peers[index].isConnected = false
            print("[HiveDemo] Connection failed to \(peerName) - keeping in list for retry")
        }
    }

    private func handleDataReceived(identifier: String, data: Data) {
        // Parse HIVE document format (Android HiveDocument compatible)
        // Header: [version: 4] [node_id: 4]
        // GCounter: [num_entries: 4] + [node_id: 4, count: 8] * N
        // Extended: [0xAB marker: 1] [reserved: 1] [peripheral_len: 2] [peripheral: M bytes]
        // Peripheral: [id: 4] [parent: 4] [type: 1] [callsign: 12] [health: 4] [has_event: 1] [event?: 9] [timestamp: 8]

        guard data.count >= 8 else { return }

        let version = data.subdata(in: 0..<4).withUnsafeBytes { $0.load(as: UInt32.self) }
        let sourceNodeId = data.subdata(in: 4..<8).withUnsafeBytes { $0.load(as: UInt32.self) }

        log("[HiveDemo] Received document v\(version) from \(String(format: "HIVE-%08X", sourceNodeId)), \(data.count) bytes")
        log("[HiveDemo] Raw data: \(data.map { String(format: "%02X", $0) }.joined(separator: " "))")

        // Add or update peer from incoming data (handles case where remote connected to us)
        if sourceNodeId != localNodeId {
            // First try to find peer by exact nodeId
            if let index = peers.firstIndex(where: { $0.nodeId == sourceNodeId }) {
                // Update existing peer
                peers[index].lastSeen = Date()
                peers[index].isConnected = true
                log("[HiveDemo] Updated existing peer \(peers[index].displayName)")
            } else if identifier != "peripheral", let index = peers.firstIndex(where: { $0.identifier == identifier }) {
                // Update peer found by BLE identifier (when we connected to them)
                log("[HiveDemo] Updating peer nodeId from \(String(format: "%08X", peers[index].nodeId)) to \(String(format: "%08X", sourceNodeId))")
                peers[index] = HivePeer(
                    identifier: identifier,
                    nodeId: sourceNodeId,
                    meshId: Self.MESH_ID,
                    advertisedName: peers[index].advertisedName,
                    isConnected: true,
                    rssi: peers[index].rssi,
                    lastSeen: Date()
                )
            } else if identifier == "peripheral" {
                // Data came from a central that connected to US - try to find the discovered peer with temp nodeId
                // Look for a peer that was discovered but has a temp nodeId (not yet confirmed by document)
                if let index = peers.firstIndex(where: { !$0.isConnected || $0.rssi != 0 }) {
                    // Found a discovered peer - update its nodeId to the real one from the document
                    let oldNodeId = peers[index].nodeId
                    log("[HiveDemo] Merging incoming peer: updating \(String(format: "%08X", oldNodeId)) -> \(String(format: "%08X", sourceNodeId))")
                    peers[index] = HivePeer(
                        identifier: peers[index].identifier,
                        nodeId: sourceNodeId,
                        meshId: Self.MESH_ID,
                        advertisedName: peers[index].advertisedName,
                        isConnected: true,
                        rssi: peers[index].rssi,
                        lastSeen: Date()
                    )
                } else {
                    // No discovered peer to merge with - add new
                    let peer = HivePeer(
                        identifier: "incoming-\(sourceNodeId)",
                        nodeId: sourceNodeId,
                        meshId: Self.MESH_ID,
                        advertisedName: String(format: "HIVE-%08X", sourceNodeId),
                        isConnected: true,
                        rssi: 0,
                        lastSeen: Date()
                    )
                    peers.append(peer)
                    log("[HiveDemo] Added peer from incoming connection: \(peer.displayName)")
                    showToast("Peer connected: \(peer.displayName)")
                }
            } else {
                // Add new peer
                let peer = HivePeer(
                    identifier: identifier,
                    nodeId: sourceNodeId,
                    meshId: Self.MESH_ID,
                    advertisedName: String(format: "HIVE-%08X", sourceNodeId),
                    isConnected: true,
                    rssi: 0,
                    lastSeen: Date()
                )
                peers.append(peer)
                log("[HiveDemo] Added peer from incoming connection: \(peer.displayName)")
                showToast("Peer connected: \(peer.displayName)")
            }
        }

        // Calculate exact position of extended marker (0xAB) after header and GCounter
        // Header: 8 bytes (version + node_id)
        // GCounter: 4 bytes (num_entries) + num_entries * 12 bytes
        guard data.count >= 12 else { return }  // Need at least header + num_entries

        let numEntries = data.subdata(in: 8..<12).withUnsafeBytes { $0.load(as: UInt32.self) }
        let counterEnd = 8 + 4 + Int(numEntries) * 12  // header + num_entries + entries

        print("[HiveDemo] Document: \(data.count) bytes, numEntries=\(numEntries), counterEnd=\(counterEnd)")

        // Check for extended marker at calculated position
        if data.count > counterEnd && data[counterEnd] == 0xAB {
            let markerIndex = counterEnd
            let peripheralLen = data.subdata(in: (markerIndex + 2)..<(markerIndex + 4)).withUnsafeBytes { $0.load(as: UInt16.self) }
            let peripheralStart = markerIndex + 4

            print("[HiveDemo] Found 0xAB marker at \(markerIndex), peripheralLen=\(peripheralLen)")

            if peripheralLen >= 26 && peripheralStart + Int(peripheralLen) <= data.count {
                // Parse HivePeripheral structure
                // [id: 4] [parent: 4] [type: 1] [callsign: 12] [health: 4] [has_event: 1] [event?: 9] [timestamp: 8]
                // has_event is at offset 25 (4+4+1+12+4)
                let hasEventOffset = peripheralStart + 25

                if hasEventOffset < data.count {
                    let hasEvent = data[hasEventOffset] != 0
                    print("[HiveDemo] Peripheral: len=\(peripheralLen), hasEvent=\(hasEvent)")

                    if hasEvent && hasEventOffset + 1 < data.count {
                        // Event is at offset 26 within peripheral
                        let eventTypeOffset = hasEventOffset + 1
                        let androidEventType = data[eventTypeOffset]
                        print("[HiveDemo] Android event type: \(androidEventType)")

                        // Map Android event types to iOS handling:
                        // Android: EMERGENCY=3, ACK=6
                        // iOS internal: 1 = Emergency, 2 = ACK
                        let iosEventType: UInt8
                        switch androidEventType {
                        case 3: iosEventType = 1  // Emergency
                        case 6: iosEventType = 2  // ACK
                        default: iosEventType = androidEventType
                        }
                        handleEventByte(iosEventType, fromNodeId: sourceNodeId)
                    }
                }
            }
        }
    }

    private func handleEventByte(_ eventByte: UInt8, fromNodeId: UInt32) {
        print("[HiveDemo] Event byte=\(eventByte) from node=\(String(format: "%08X", fromNodeId))")
        print("[HiveDemo] Known peers: \(peers.map { String(format: "%08X", $0.nodeId) })")

        // Event types from HIVE protocol
        switch eventByte {
        case 1: // Emergency
            print("[HiveDemo] EMERGENCY event received!")
            if let peer = peers.first(where: { $0.nodeId == fromNodeId }) {
                handleEmergencyReceived(from: peer)
            } else {
                // Create temporary peer for emergency from unknown source
                print("[HiveDemo] Emergency from unknown peer, creating temp peer")
                let tempPeer = HivePeer(
                    identifier: "emergency-\(fromNodeId)",
                    nodeId: fromNodeId,
                    meshId: Self.MESH_ID,
                    advertisedName: String(format: "HIVE-%08X", fromNodeId),
                    isConnected: true,
                    rssi: 0,
                    lastSeen: Date()
                )
                peers.append(tempPeer)
                handleEmergencyReceived(from: tempPeer)
            }
        case 2: // ACK
            print("[HiveDemo] ACK event received!")
            if let peer = peers.first(where: { $0.nodeId == fromNodeId }) {
                handleAckReceived(from: peer)
            } else {
                // Handle ACK from peer we may not have tracked
                print("[HiveDemo] ACK from node \(String(format: "%08X", fromNodeId))")
                ackStatus.pendingAcks[fromNodeId] = true
                showToast("✓ ACK from \(String(format: "HIVE-%08X", fromNodeId))")
                checkAllAcked()
            }
        default:
            print("[HiveDemo] Unknown event type: \(eventByte)")
        }
    }

    private func cleanupStalePeers() {
        let staleThreshold = Date().addingTimeInterval(-60) // 60 seconds
        let staleCount = peers.filter { !$0.isConnected && $0.lastSeen < staleThreshold }.count
        if staleCount > 0 {
            print("[HiveDemo] Cleaning up \(staleCount) stale peers")
            peers.removeAll { !$0.isConnected && $0.lastSeen < staleThreshold }
        }
    }

    // MARK: - Event Handling

    /// Build a HIVE document compatible with Android HiveDocument format
    /// Format:
    /// - Header: [version: 4] [node_id: 4]
    /// - GCounter: [num_entries: 4] + [node_id: 4, count: 8] * N
    /// - Extended: [0xAB marker: 1] [reserved: 1] [peripheral_len: 2] [peripheral: M bytes]
    /// - Peripheral: [id: 4] [parent: 4] [type: 1] [callsign: 12] [health: 4] [has_event: 1] [event?: 9] [timestamp: 8]
    private func buildHiveDocument(eventType: UInt8) -> Data {
        var data = Data()

        // === Header (8 bytes) ===
        // Version (4 bytes, little-endian)
        var version: UInt32 = 1
        data.append(Data(bytes: &version, count: 4))

        // Node ID (4 bytes, little-endian)
        var nodeId = localNodeId
        data.append(Data(bytes: &nodeId, count: 4))

        // === GCounter (4 bytes for empty counter) ===
        var numEntries: UInt32 = 0
        data.append(Data(bytes: &numEntries, count: 4))

        // === Extended section with HivePeripheral ===
        // Event marker (0xAB)
        data.append(0xAB)

        // Reserved (1 byte)
        data.append(0x00)

        // Build HivePeripheral data
        let hasEvent = eventType != 0
        let peripheralSize: UInt16 = hasEvent ? 43 : 34
        var peripheralLen = peripheralSize
        data.append(Data(bytes: &peripheralLen, count: 2))

        // Peripheral data starts here
        // id (4 bytes)
        var peripheralId = localNodeId
        data.append(Data(bytes: &peripheralId, count: 4))

        // parentNode (4 bytes)
        var parentNode: UInt32 = 0
        data.append(Data(bytes: &parentNode, count: 4))

        // peripheralType (1 byte) - SOLDIER_SENSOR = 1
        data.append(1)

        // callsign (12 bytes, null-padded)
        var callsignBytes = "SWIFT".data(using: .utf8) ?? Data()
        callsignBytes.append(contentsOf: [UInt8](repeating: 0, count: 12 - callsignBytes.count))
        data.append(callsignBytes.prefix(12))

        // health (4 bytes: battery, heartRate, activity, alerts)
        data.append(contentsOf: [100, 0, 0, 0] as [UInt8])

        // has_event (1 byte)
        data.append(hasEvent ? 1 : 0)

        // event (9 bytes if present): eventType (1 byte) + timestamp (8 bytes)
        if hasEvent {
            // Map iOS event types to Android:
            // iOS: 1 = Emergency, 2 = ACK
            // Android: EMERGENCY=3, ACK=6
            let androidEventType: UInt8
            switch eventType {
            case 1: androidEventType = 3  // Emergency
            case 2: androidEventType = 6  // ACK
            default: androidEventType = eventType
            }
            data.append(androidEventType)

            // timestamp (8 bytes, little-endian)
            var timestamp: UInt64 = UInt64(Date().timeIntervalSince1970 * 1000)
            data.append(Data(bytes: &timestamp, count: 8))
        }

        // timestamp (8 bytes, little-endian) - peripheral timestamp
        var peripheralTimestamp: UInt64 = UInt64(Date().timeIntervalSince1970 * 1000)
        data.append(Data(bytes: &peripheralTimestamp, count: 8))

        print("[HiveDemo] Built document: \(data.count) bytes")
        print("[HiveDemo] Document hex: \(data.map { String(format: "%02X", $0) }.joined(separator: " "))")

        return data
    }

    /// Send an emergency alert to all peers
    func sendEmergency() {
        guard isMeshActive else {
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

        // Build and send emergency document (event type 1)
        let document = buildHiveDocument(eventType: 1)
        bleManager?.sendData(document)

        showToast("🚨 EMERGENCY SENT!")
        statusMessage = "⚠️ EMERGENCY - TAP ACK"
    }

    /// Send an ACK
    func sendAck() {
        guard isMeshActive else {
            showToast("Mesh not active")
            return
        }

        print("[HiveDemo] >>> SENDING ACK")

        // Build and send ACK document (event type 2)
        let document = buildHiveDocument(eventType: 2)
        bleManager?.sendData(document)

        ackStatus.pendingAcks[localNodeId] = true
        showToast("✓ ACK sent")

        checkAllAcked()
    }

    /// Reset the alert state
    func resetAlert() {
        print("[HiveDemo] >>> RESETTING ALERT")

        ackStatus.reset()
        statusMessage = "Mesh active - \(localDisplayName)"
        showToast("Alert reset")
    }

    /// Handle emergency received from a peer
    func handleEmergencyReceived(from peer: HivePeer) {
        print("[HiveDemo] Received EMERGENCY from \(peer.displayName)")

        // Initialize ACK tracking
        ackStatus.pendingAcks.removeAll()
        for p in peers {
            ackStatus.pendingAcks[p.nodeId] = false
        }
        ackStatus.pendingAcks[localNodeId] = false  // We haven't acked yet
        ackStatus.pendingAcks[peer.nodeId] = true   // Source has implicitly acked
        ackStatus.emergencySourceNodeId = peer.nodeId

        showToast("🚨 EMERGENCY from \(peer.displayName)!")
        statusMessage = "⚠️ EMERGENCY - TAP ACK"

        triggerVibration()
    }

    /// Handle ACK received from a peer
    func handleAckReceived(from peer: HivePeer) {
        print("[HiveDemo] Received ACK from \(peer.displayName)")

        ackStatus.pendingAcks[peer.nodeId] = true
        showToast("✓ ACK from \(peer.displayName)")

        checkAllAcked()
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
