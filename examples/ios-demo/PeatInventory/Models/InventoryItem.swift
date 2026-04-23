import Foundation

struct InventoryItem: Identifiable, Codable, Hashable, Sendable {
    let id: UUID
    var nsn: String
    var nomenclature: String
    var serialNumber: String
    var quantity: Int
    var conditionCode: ConditionCode
    var location: String
    var responsibleUnit: String
    var responsiblePerson: String
    var category: EquipmentCategory
    var notes: String
    var lastModified: Date
    var modifiedBy: String

    init(
        id: UUID = UUID(),
        nsn: String = "",
        nomenclature: String = "",
        serialNumber: String = "",
        quantity: Int = 1,
        conditionCode: ConditionCode = .A,
        location: String = "",
        responsibleUnit: String = "",
        responsiblePerson: String = "",
        category: EquipmentCategory = .other,
        notes: String = "",
        lastModified: Date = Date(),
        modifiedBy: String = ""
    ) {
        self.id = id
        self.nsn = nsn
        self.nomenclature = nomenclature
        self.serialNumber = serialNumber
        self.quantity = quantity
        self.conditionCode = conditionCode
        self.location = location
        self.responsibleUnit = responsibleUnit
        self.responsiblePerson = responsiblePerson
        self.category = category
        self.notes = notes
        self.lastModified = lastModified
        self.modifiedBy = modifiedBy
    }
}

enum ConditionCode: String, Codable, CaseIterable, Identifiable, Sendable {
    case A = "Serviceable"
    case B = "Serviceable - Needs Repair"
    case C = "Priority Repair"
    case D = "Requires Overhaul"
    case F = "Unserviceable - Reparable"
    case H = "Condemned"

    var id: String { rawValue }

    var shortLabel: String {
        switch self {
        case .A: "A"
        case .B: "B"
        case .C: "C"
        case .D: "D"
        case .F: "F"
        case .H: "H"
        }
    }
}

enum EquipmentCategory: String, Codable, CaseIterable, Identifiable, Sendable {
    case communications = "Communications"
    case optics = "Optics"
    case weapons = "Weapons"
    case vehicles = "Vehicles"
    case medical = "Medical"
    case powerAndElectrical = "Power & Electrical"
    case other = "Other"

    var id: String { rawValue }

    var sfSymbol: String {
        switch self {
        case .communications: "antenna.radiowaves.left.and.right"
        case .optics: "scope"
        case .weapons: "shield.fill"
        case .vehicles: "car.fill"
        case .medical: "cross.case.fill"
        case .powerAndElectrical: "bolt.fill"
        case .other: "shippingbox.fill"
        }
    }
}
