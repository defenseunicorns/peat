import SwiftUI

struct ItemDetailView: View {
    let viewModel: InventoryViewModel
    let item: InventoryItem
    @State private var showingEdit = false

    /// Live version of the item from the view model, so CRDT updates reflect immediately
    private var currentItem: InventoryItem {
        viewModel.items.first(where: { $0.id == item.id }) ?? item
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                headerSection
                Divider()
                identificationSection
                Divider()
                assignmentSection
                Divider()
                statusSection
                if !currentItem.notes.isEmpty {
                    Divider()
                    notesSection
                }
                Divider()
                metadataSection
            }
            .padding()
        }
        .navigationTitle(currentItem.nomenclature)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showingEdit = true
                } label: {
                    Text("Edit")
                        .foregroundStyle(MilitaryTheme.odGreen)
                }
            }
        }
        .sheet(isPresented: $showingEdit) {
            ItemEditView(viewModel: viewModel, item: currentItem)
        }
    }

    private var headerSection: some View {
        HStack(spacing: 16) {
            Image(systemName: currentItem.category.sfSymbol)
                .font(.largeTitle)
                .foregroundStyle(MilitaryTheme.odGreen)
                .frame(width: 60, height: 60)
                .background(MilitaryTheme.odGreen.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 12))

            VStack(alignment: .leading, spacing: 4) {
                Text(currentItem.nomenclature)
                    .font(.headline)
                Text(currentItem.category.rawValue)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                HStack {
                    ConditionBadge(code: currentItem.conditionCode)
                    Text("Qty: \(currentItem.quantity)")
                        .font(.subheadline.bold())
                        .foregroundStyle(MilitaryTheme.odGreen)
                }
            }
        }
    }

    private var identificationSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "Identification", sfSymbol: "number")
            DetailRow(label: "NSN", value: currentItem.nsn, monospaced: true)
            DetailRow(label: "Serial Number", value: currentItem.serialNumber, monospaced: true)
            DetailRow(label: "Category", value: currentItem.category.rawValue)
        }
    }

    private var assignmentSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "Assignment", sfSymbol: "person.fill")
            DetailRow(label: "Responsible Person", value: currentItem.responsiblePerson)
            DetailRow(label: "Responsible Unit", value: currentItem.responsibleUnit)
            DetailRow(label: "Location", value: currentItem.location)
        }
    }

    private var statusSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "Status", sfSymbol: "chart.bar.fill")
            HStack {
                Text("Condition")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Spacer()
                HStack(spacing: 8) {
                    ConditionBadgeLarge(code: currentItem.conditionCode)
                    Text(currentItem.conditionCode.rawValue)
                        .font(.subheadline)
                }
            }
            quantityAdjuster
        }
    }

    private var quantityAdjuster: some View {
        HStack {
            Text("Quantity")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Spacer()
            HStack(spacing: 12) {
                Button {
                    Task { await viewModel.adjustQuantity(for: currentItem, by: -1) }
                } label: {
                    Image(systemName: "minus.circle.fill")
                        .font(.title2)
                        .foregroundStyle(currentItem.quantity > 0 ? MilitaryTheme.odGreen : .gray)
                }
                .disabled(currentItem.quantity <= 0)

                Text("\(currentItem.quantity)")
                    .font(.headline.monospacedDigit())
                    .frame(minWidth: 36)

                Button {
                    Task { await viewModel.adjustQuantity(for: currentItem, by: 1) }
                } label: {
                    Image(systemName: "plus.circle.fill")
                        .font(.title2)
                        .foregroundStyle(MilitaryTheme.odGreen)
                }
            }
        }
    }

    private var notesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeader(title: "Notes", sfSymbol: "note.text")
            Text(currentItem.notes)
                .font(.body)
                .foregroundStyle(.secondary)
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(.systemGray6))
                .clipShape(RoundedRectangle(cornerRadius: 8))
        }
    }

    private var metadataSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeader(title: "Sync Metadata", sfSymbol: "clock.fill")
            DetailRow(label: "Last Modified", value: currentItem.lastModified.formatted(date: .abbreviated, time: .shortened))
            DetailRow(label: "Modified By", value: currentItem.modifiedBy.isEmpty ? "Local" : currentItem.modifiedBy)
            DetailRow(label: "Item ID", value: currentItem.id.uuidString.prefix(8).description, monospaced: true)
        }
    }
}

struct SectionHeader: View {
    let title: String
    let sfSymbol: String

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: sfSymbol)
                .foregroundStyle(MilitaryTheme.odGreen)
            Text(title)
                .font(.headline)
        }
    }
}

struct DetailRow: View {
    let label: String
    let value: String
    var monospaced: Bool = false

    var body: some View {
        HStack {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(monospaced ? .subheadline.monospaced() : .subheadline)
                .multilineTextAlignment(.trailing)
        }
    }
}
