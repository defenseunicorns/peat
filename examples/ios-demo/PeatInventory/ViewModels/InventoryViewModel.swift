import Foundation
import SwiftUI

@Observable
final class InventoryViewModel {
    var items: [InventoryItem] = []
    var searchText: String = ""
    var selectedCategory: EquipmentCategory?
    var selectedCondition: ConditionCode?
    var selectedUnit: String?
    var isLoading: Bool = false
    var errorMessage: String?

    private let service: PeatServiceProtocol

    init(service: PeatServiceProtocol) {
        self.service = service
    }

    var filteredItems: [InventoryItem] {
        var result = items

        if !searchText.isEmpty {
            let query = searchText.lowercased()
            result = result.filter {
                $0.nomenclature.lowercased().contains(query) ||
                $0.nsn.contains(query) ||
                $0.serialNumber.lowercased().contains(query) ||
                $0.responsiblePerson.lowercased().contains(query) ||
                $0.location.lowercased().contains(query)
            }
        }

        if let category = selectedCategory {
            result = result.filter { $0.category == category }
        }

        if let condition = selectedCondition {
            result = result.filter { $0.conditionCode == condition }
        }

        if let unit = selectedUnit {
            result = result.filter { $0.responsibleUnit == unit }
        }

        return result
    }

    var uniqueUnits: [String] {
        Array(Set(items.map(\.responsibleUnit))).sorted()
    }

    var itemCountByCategory: [(EquipmentCategory, Int)] {
        EquipmentCategory.allCases.compactMap { cat in
            let count = items.filter { $0.category == cat }.count
            return count > 0 ? (cat, count) : nil
        }
    }

    var itemCountByCondition: [(ConditionCode, Int)] {
        ConditionCode.allCases.compactMap { code in
            let count = items.filter { $0.conditionCode == code }.count
            return count > 0 ? (code, count) : nil
        }
    }

    func loadItems() async {
        isLoading = true
        items = await service.getAllItems()
        isLoading = false

        service.onRemoteChange { [weak self] updatedItems in
            Task { @MainActor in
                self?.items = updatedItems
            }
        }
    }

    func saveItem(_ item: InventoryItem) async {
        do {
            try await service.putItem(item)
            items = await service.getAllItems()
        } catch {
            errorMessage = "Failed to save: \(error.localizedDescription)"
        }
    }

    func deleteItem(_ item: InventoryItem) async {
        do {
            try await service.deleteItem(id: item.id)
            items = await service.getAllItems()
        } catch {
            errorMessage = "Failed to delete: \(error.localizedDescription)"
        }
    }

    func deleteItems(at offsets: IndexSet) async {
        let toDelete = offsets.map { filteredItems[$0] }
        for item in toDelete {
            await deleteItem(item)
        }
    }

    func loadDemoData() async {
        for item in DemoDataService.sampleItems {
            try? await service.putItem(item)
        }
        items = await service.getAllItems()
    }

    func clearAllData() async {
        for item in items {
            try? await service.deleteItem(id: item.id)
        }
        items = []
    }

    func adjustQuantity(for item: InventoryItem, by delta: Int) async {
        var updated = item
        updated.quantity = max(0, item.quantity + delta)
        updated.lastModified = Date()
        await saveItem(updated)
    }

    func clearFilters() {
        selectedCategory = nil
        selectedCondition = nil
        selectedUnit = nil
    }

    var hasActiveFilters: Bool {
        selectedCategory != nil || selectedCondition != nil || selectedUnit != nil
    }
}
