import Foundation

actor PersistenceService {
    private let fileURL: URL

    init() {
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
        self.fileURL = docs.appendingPathComponent("peat_inventory.json")
    }

    func loadItems() -> [InventoryItem] {
        guard FileManager.default.fileExists(atPath: fileURL.path),
              let data = try? Data(contentsOf: fileURL),
              let items = try? JSONDecoder.peatDecoder.decode([InventoryItem].self, from: data) else {
            return []
        }
        return items
    }

    func saveItems(_ items: [InventoryItem]) {
        guard let data = try? JSONEncoder.peatEncoder.encode(items) else { return }
        try? data.write(to: fileURL, options: .atomic)
    }
}

extension JSONEncoder {
    static let peatEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = .prettyPrinted
        return encoder
    }()
}

extension JSONDecoder {
    static let peatDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
