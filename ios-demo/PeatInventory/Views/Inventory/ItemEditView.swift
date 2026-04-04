import SwiftUI

struct ItemEditView: View {
    let viewModel: InventoryViewModel
    let item: InventoryItem?
    @Environment(\.dismiss) private var dismiss

    @State private var nsn: String = ""
    @State private var nomenclature: String = ""
    @State private var serialNumber: String = ""
    @State private var quantity: Int = 1
    @State private var conditionCode: ConditionCode = .A
    @State private var location: String = ""
    @State private var responsibleUnit: String = ""
    @State private var responsiblePerson: String = ""
    @State private var category: EquipmentCategory = .other
    @State private var notes: String = ""

    private var isEditing: Bool { item != nil }

    var body: some View {
        NavigationStack {
            Form {
                Section("Identification") {
                    TextField("NSN (e.g., 5820-01-451-8250)", text: $nsn)
                        .font(.body.monospaced())
                        .textInputAutocapitalization(.characters)
                    TextField("Nomenclature", text: $nomenclature)
                        .textInputAutocapitalization(.characters)
                    TextField("Serial Number", text: $serialNumber)
                        .font(.body.monospaced())
                }

                Section("Classification") {
                    Picker("Category", selection: $category) {
                        ForEach(EquipmentCategory.allCases) { cat in
                            Label(cat.rawValue, systemImage: cat.sfSymbol)
                                .tag(cat)
                        }
                    }

                    VStack(alignment: .leading, spacing: 8) {
                        Text("Condition Code")
                            .font(.subheadline)
                        Picker("Condition", selection: $conditionCode) {
                            ForEach(ConditionCode.allCases) { code in
                                Text("\(code.shortLabel) — \(code.rawValue)")
                                    .tag(code)
                            }
                        }
                        .pickerStyle(.segmented)

                        Text(conditionCode.rawValue)
                            .font(.caption)
                            .foregroundStyle(conditionCode.color)
                    }

                    Stepper("Quantity: \(quantity)", value: $quantity, in: 1...9999)
                }

                Section("Assignment") {
                    TextField("Location / Grid Reference", text: $location)
                    TextField("Responsible Unit", text: $responsibleUnit)
                    TextField("Responsible Person", text: $responsiblePerson)
                }

                Section("Notes") {
                    TextEditor(text: $notes)
                        .frame(minHeight: 80)
                }
            }
            .navigationTitle(isEditing ? "Edit Item" : "Add Item")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        saveItem()
                    }
                    .bold()
                    .foregroundStyle(MilitaryTheme.odGreen)
                    .disabled(nomenclature.isEmpty || nsn.isEmpty)
                }
            }
            .onAppear {
                if let item {
                    nsn = item.nsn
                    nomenclature = item.nomenclature
                    serialNumber = item.serialNumber
                    quantity = item.quantity
                    conditionCode = item.conditionCode
                    location = item.location
                    responsibleUnit = item.responsibleUnit
                    responsiblePerson = item.responsiblePerson
                    category = item.category
                    notes = item.notes
                }
            }
        }
    }

    private func saveItem() {
        let newItem = InventoryItem(
            id: item?.id ?? UUID(),
            nsn: nsn.trimmingCharacters(in: .whitespaces),
            nomenclature: nomenclature.trimmingCharacters(in: .whitespaces),
            serialNumber: serialNumber.trimmingCharacters(in: .whitespaces),
            quantity: quantity,
            conditionCode: conditionCode,
            location: location.trimmingCharacters(in: .whitespaces),
            responsibleUnit: responsibleUnit.trimmingCharacters(in: .whitespaces),
            responsiblePerson: responsiblePerson.trimmingCharacters(in: .whitespaces),
            category: category,
            notes: notes.trimmingCharacters(in: .whitespaces),
            lastModified: Date(),
            modifiedBy: ""
        )
        Task {
            await viewModel.saveItem(newItem)
            dismiss()
        }
    }
}
