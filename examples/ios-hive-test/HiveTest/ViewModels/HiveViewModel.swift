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
    private var hiveService: CBMutableService?
    private var syncDataCharacteristic: CBMutableCharacteristic?

    /// Local node ID and device name for advertising
    var localNodeId: UInt32 = 0
    var localDeviceName: String = "HIVE-00000000"

    var onStateChanged: ((CBManagerState) -> Void)?
    var onPeerDiscovered: ((String, String?, Int, Data?) -> Void)?  // identifier, name, rssi, manufacturerData
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
        print("[BLE Peripheral] Read request for \(request.characteristic.uuid)")

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
            print("[BLE Peripheral] Write request: \(request.value?.count ?? 0) bytes")

            if let data = request.value {
                // Notify the app of received data
                onDataReceived?("peripheral", data)
            }

            peripheral.respond(to: request, withResult: .success)
        }
    }

    // MARK: - Central (Scanning) Mode

    func startScanning() {
        guard centralManager.state == .poweredOn else {
            print("[BLE] Cannot scan - Bluetooth not powered on")
            return
        }

        print("[BLE] Starting scan for HIVE service \(HIVE_SERVICE_UUID)")
        // Scan for devices advertising HIVE service, or nil to scan all
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

        // Get manufacturer data (contains node ID)
        let manufacturerData = advertisementData[CBAdvertisementDataManufacturerDataKey] as? Data

        // Store peripheral reference for connection
        discoveredPeripherals[identifier] = peripheral

        print("[BLE] Discovered: \(name ?? "Unknown") RSSI=\(rssi) mfg=\(manufacturerData?.count ?? 0) bytes")
        onPeerDiscovered?(identifier, name, rssi, manufacturerData)
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        print("[BLE] Connected to \(peripheral.name ?? peripheral.identifier.uuidString)")
        peripheral.delegate = self
        peripheral.discoverServices([HIVE_SERVICE_UUID])
        onPeerConnected?(peripheral.identifier.uuidString)
    }

    func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        print("[BLE] Disconnected from \(peripheral.name ?? peripheral.identifier.uuidString)")
        onPeerDisconnected?(peripheral.identifier.uuidString)
    }

    var onConnectionFailed: ((String) -> Void)?

    func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        print("[BLE] Failed to connect: \(error?.localizedDescription ?? "unknown")")
        onConnectionFailed?(peripheral.identifier.uuidString)
    }

    // MARK: - CBPeripheralDelegate

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let services = peripheral.services else { return }
        for service in services {
            print("[BLE] Discovered service: \(service.uuid)")
            if service.uuid == HIVE_SERVICE_UUID {
                peripheral.discoverCharacteristics([HIVE_DOC_CHAR_UUID], for: service)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        guard let characteristics = service.characteristics else { return }
        for char in characteristics {
            print("[BLE] Discovered characteristic: \(char.uuid)")
            if char.uuid == HIVE_DOC_CHAR_UUID {
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

        bleManager?.onPeerDiscovered = { [weak self] identifier, name, rssi, mfgData in
            Task { @MainActor [weak self] in
                self?.handlePeerDiscovered(identifier: identifier, name: name, rssi: rssi, manufacturerData: mfgData)
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

        // Cleanup stale peers periodically
        peerCleanupTimer = Timer.scheduledTimer(withTimeInterval: 10.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.cleanupStalePeers()
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

    private func handlePeerDiscovered(identifier: String, name: String?, rssi: Int, manufacturerData: Data?) {
        // Parse mesh ID and node ID from name using Rust hive-btle via UniFFI
        var nodeId: UInt32 = 0
        var meshId: String? = nil

        if let name = name, let parsed = parseHiveDeviceName(name: name) {
            // Successfully parsed using Rust MeshConfig::parse_device_name()
            meshId = parsed.meshId
            nodeId = parsed.nodeId
        }

        // If no node ID from name, try manufacturer data
        if nodeId == 0, let mfgData = manufacturerData, mfgData.count >= 4 {
            // Skip 2-byte company ID, read 4-byte node ID
            if mfgData.count >= 6 {
                nodeId = mfgData.subdata(in: 2..<6).withUnsafeBytes { $0.load(as: UInt32.self) }
            }
        }

        // Generate node ID from identifier if still zero
        if nodeId == 0 {
            nodeId = UInt32(identifier.hashValue & 0xFFFFFFFF)
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

        // Auto-connect to new HIVE peers in SAME MESH only
        // Don't connect to ourselves!
        if isNewPeer && nodeId != localNodeId {
            // Check mesh ID match using Rust matchesMesh() via UniFFI
            // Returns true if same mesh or if legacy format (nil mesh ID)
            let sameMesh = matchesMesh(ourMeshId: Self.MESH_ID, deviceMeshId: meshId)
            if sameMesh {
                print("[HiveDemo] Auto-connecting to \(String(format: "HIVE-%08X", nodeId)) (mesh: \(meshId ?? "any"))...")
                bleManager?.connect(identifier: identifier)
            } else {
                print("[HiveDemo] Skipping peer \(String(format: "HIVE-%08X", nodeId)) - different mesh (\(meshId ?? "?") != \(Self.MESH_ID))")
            }
        } else if nodeId == localNodeId {
            print("[HiveDemo] Skipping self-connection to \(String(format: "HIVE-%08X", nodeId))")
            // Remove ourselves from the peer list
            peers.removeAll { $0.nodeId == localNodeId }
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
            peers.remove(at: index)
            showToast("Disconnected from \(peerName)")
            print("[HiveDemo] Removed peer \(peerName) - will re-add if discovered again")
        }
    }

    private func handleConnectionFailed(identifier: String) {
        if let index = peers.firstIndex(where: { $0.identifier == identifier }) {
            let peerName = peers[index].displayName
            peers.remove(at: index)
            print("[HiveDemo] Connection failed to \(peerName) - removed from list")
        }
    }

    private func handleDataReceived(identifier: String, data: Data) {
        // Parse HIVE document format
        // [version: 4 bytes] [node_id: 4 bytes] [counter_data: N bytes] [0xAB marker] [reserved: 1 byte] [peripheral_len: 2 bytes] [peripheral_data: M bytes]

        guard data.count >= 8 else { return }

        let version = data.subdata(in: 0..<4).withUnsafeBytes { $0.load(as: UInt32.self) }
        let sourceNodeId = data.subdata(in: 4..<8).withUnsafeBytes { $0.load(as: UInt32.self) }

        print("[HiveDemo] Received document v\(version) from \(String(format: "HIVE-%08X", sourceNodeId))")

        // Look for event marker (0xAB)
        if let markerIndex = data.firstIndex(of: 0xAB), markerIndex + 4 < data.count {
            let peripheralLen = data.subdata(in: (markerIndex + 2)..<(markerIndex + 4)).withUnsafeBytes { $0.load(as: UInt16.self) }
            if peripheralLen > 0 && markerIndex + 4 + Int(peripheralLen) <= data.count {
                let eventByte = data[markerIndex + 4]
                handleEventByte(eventByte, fromNodeId: sourceNodeId)
            }
        }
    }

    private func handleEventByte(_ eventByte: UInt8, fromNodeId: UInt32) {
        // Event types from HIVE protocol
        switch eventByte {
        case 1: // Emergency
            if let peer = peers.first(where: { $0.nodeId == fromNodeId }) {
                handleEmergencyReceived(from: peer)
            }
        case 2: // ACK
            if let peer = peers.first(where: { $0.nodeId == fromNodeId }) {
                handleAckReceived(from: peer)
            }
        default:
            break
        }
    }

    private func cleanupStalePeers() {
        let staleThreshold = Date().addingTimeInterval(-30) // 30 seconds
        peers.removeAll { !$0.isConnected && $0.lastSeen < staleThreshold }
    }

    // MARK: - Event Handling

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

        // TODO: Send via GATT write to connected peers

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

        // TODO: Send via GATT write to connected peers

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
