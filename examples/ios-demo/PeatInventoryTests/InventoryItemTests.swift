import XCTest
@testable import PeatInventory

final class InventoryItemTests: XCTestCase {

    func testItemCreation() {
        let item = InventoryItem(
            nsn: "5820-01-451-8250",
            nomenclature: "RADIO SET, AN/PRC-152A",
            serialNumber: "W925692",
            quantity: 4,
            conditionCode: .A,
            location: "CP ALPHA",
            responsibleUnit: "1st PLT, A CO",
            responsiblePerson: "SGT Torres",
            category: .communications
        )

        XCTAssertEqual(item.nsn, "5820-01-451-8250")
        XCTAssertEqual(item.conditionCode, .A)
        XCTAssertEqual(item.category, .communications)
        XCTAssertEqual(item.quantity, 4)
    }

    func testItemCodable() throws {
        let item = InventoryItem(
            nsn: "5855-01-432-0524",
            nomenclature: "NIGHT VISION DEVICE, AN/PVS-14",
            serialNumber: "NV20451",
            quantity: 12,
            conditionCode: .B,
            category: .optics
        )

        let data = try JSONEncoder.peatEncoder.encode(item)
        let decoded = try JSONDecoder.peatDecoder.decode(InventoryItem.self, from: data)

        XCTAssertEqual(decoded.id, item.id)
        XCTAssertEqual(decoded.nsn, item.nsn)
        XCTAssertEqual(decoded.nomenclature, item.nomenclature)
        XCTAssertEqual(decoded.conditionCode, .B)
        XCTAssertEqual(decoded.category, .optics)
        XCTAssertEqual(decoded.quantity, 12)
    }

    func testConditionCodes() {
        XCTAssertEqual(ConditionCode.A.shortLabel, "A")
        XCTAssertEqual(ConditionCode.A.rawValue, "Serviceable")
        XCTAssertEqual(ConditionCode.H.shortLabel, "H")
        XCTAssertEqual(ConditionCode.H.rawValue, "Condemned")
        XCTAssertEqual(ConditionCode.allCases.count, 6)
    }

    func testEquipmentCategories() {
        XCTAssertEqual(EquipmentCategory.allCases.count, 7)
        for category in EquipmentCategory.allCases {
            XCTAssertFalse(category.sfSymbol.isEmpty)
            XCTAssertFalse(category.rawValue.isEmpty)
        }
    }

    func testDemoDataService() {
        let items = DemoDataService.sampleItems
        XCTAssertGreaterThanOrEqual(items.count, 25)

        // Verify all categories represented
        let categories = Set(items.map(\.category))
        XCTAssertTrue(categories.contains(.communications))
        XCTAssertTrue(categories.contains(.optics))
        XCTAssertTrue(categories.contains(.weapons))
        XCTAssertTrue(categories.contains(.vehicles))
        XCTAssertTrue(categories.contains(.medical))
        XCTAssertTrue(categories.contains(.powerAndElectrical))

        // Verify multiple condition codes
        let conditions = Set(items.map(\.conditionCode))
        XCTAssertTrue(conditions.count > 1)
    }

    func testItemHashable() {
        let item1 = InventoryItem(nsn: "1234", nomenclature: "Test")
        let item2 = InventoryItem(nsn: "5678", nomenclature: "Other")
        let set: Set<InventoryItem> = [item1, item2, item1]
        XCTAssertEqual(set.count, 2)
    }
}
