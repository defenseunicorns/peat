import SwiftUI

struct ItemDetailView: View {
    let viewModel: InventoryViewModel
    let item: InventoryItem
    @State private var showingEdit = false

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
                if !item.notes.isEmpty {
                    Divider()
                    notesSection
                }
                Divider()
                metadataSection
            }
            .padding()
        }
        .navigationTitle(item.nomenclature)
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
            ItemEditView(viewModel: viewModel, item: item)
        }
    }

    private var headerSection: some View {
        HStack(spacing: 16) {
            Image(systemName: item.category.sfSymbol)
                .font(.largeTitle)
                .foregroundStyle(MilitaryTheme.odGreen)
                .frame(width: 60, height: 60)
                .background(MilitaryTheme.odGreen.opacity(0.1))
                .clipShape(RoundedRectangle(cornerRadius: 12))

            VStack(alignment: .leading, spacing: 4) {
                Text(item.nomenclature)
                    .font(.headline)
                Text(item.category.rawValue)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                HStack {
                    ConditionBadge(code: item.conditionCode)
                    Text("Qty: \(item.quantity)")
                        .font(.subheadline.bold())
                        .foregroundStyle(MilitaryTheme.odGreen)
                }
            }
        }
    }

    private var identificationSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "Identification", sfSymbol: "number")
            DetailRow(label: "NSN", value: item.nsn, monospaced: true)
            DetailRow(label: "Serial Number", value: item.serialNumber, monospaced: true)
            DetailRow(label: "Category", value: item.category.rawValue)
        }
    }

    private var assignmentSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "Assignment", sfSymbol: "person.fill")
            DetailRow(label: "Responsible Person", value: item.responsiblePerson)
            DetailRow(label: "Responsible Unit", value: item.responsibleUnit)
            DetailRow(label: "Location", value: item.location)
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
                    ConditionBadgeLarge(code: item.conditionCode)
                    Text(item.conditionCode.rawValue)
                        .font(.subheadline)
                }
            }
            DetailRow(label: "Quantity", value: "\(item.quantity)")
        }
    }

    private var notesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionHeader(title: "Notes", sfSymbol: "note.text")
            Text(item.notes)
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
            DetailRow(label: "Last Modified", value: item.lastModified.formatted(date: .abbreviated, time: .shortened))
            DetailRow(label: "Modified By", value: item.modifiedBy.isEmpty ? "Local" : item.modifiedBy)
            DetailRow(label: "Item ID", value: item.id.uuidString.prefix(8).description, monospaced: true)
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
