import XCTest
@testable import PeatInventory

final class MeshModelTests: XCTestCase {

    func testPeerInfoCreation() {
        let peer = PeerInfo(
            id: "abc123",
            name: "SGT Torres",
            nodeId: "abc123",
            addresses: ["192.168.1.5"],
            transportType: .wifi,
            signalStrength: .strong,
            lastSeen: Date(),
            isCellMember: true
        )

        XCTAssertEqual(peer.name, "SGT Torres")
        XCTAssertEqual(peer.transportType, .wifi)
        XCTAssertTrue(peer.isCellMember)
    }

    func testSignalStrengthBars() {
        XCTAssertEqual(SignalStrength.strong.bars, 3)
        XCTAssertEqual(SignalStrength.medium.bars, 2)
        XCTAssertEqual(SignalStrength.weak.bars, 1)
        XCTAssertEqual(SignalStrength.unknown.bars, 0)
    }

    func testSyncStatus() {
        XCTAssertEqual(SyncStatus.synced.rawValue, "Synced")
        XCTAssertEqual(SyncStatus.offline.rawValue, "Offline")
        XCTAssertFalse(SyncStatus.synced.sfSymbol.isEmpty)
    }

    func testTransportType() {
        XCTAssertEqual(TransportType.allCases.count, 3)
        for transport in TransportType.allCases {
            XCTAssertFalse(transport.sfSymbol.isEmpty)
            XCTAssertFalse(transport.rawValue.isEmpty)
        }
    }

    func testDemoScenarios() {
        XCTAssertEqual(DemoScenario.allCases.count, 4)
        for scenario in DemoScenario.allCases {
            XCTAssertFalse(scenario.title.isEmpty)
            XCTAssertFalse(scenario.description.isEmpty)
            XCTAssertFalse(scenario.sfSymbol.isEmpty)
        }
    }
}
