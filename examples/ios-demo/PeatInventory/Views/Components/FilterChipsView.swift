import SwiftUI

struct FilterChipsView: View {
    @Bindable var viewModel: InventoryViewModel

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                if viewModel.hasActiveFilters {
                    Button {
                        viewModel.clearFilters()
                    } label: {
                        Label("Clear", systemImage: "xmark.circle.fill")
                            .font(.caption)
                    }
                    .buttonStyle(.bordered)
                    .tint(MilitaryTheme.alertRed)
                }

                Menu {
                    Button("All Categories") { viewModel.selectedCategory = nil }
                    Divider()
                    ForEach(EquipmentCategory.allCases) { cat in
                        Button {
                            viewModel.selectedCategory = cat
                        } label: {
                            Label(cat.rawValue, systemImage: cat.sfSymbol)
                        }
                    }
                } label: {
                    FilterChip(
                        label: viewModel.selectedCategory?.rawValue ?? "Category",
                        isActive: viewModel.selectedCategory != nil,
                        sfSymbol: "square.grid.2x2"
                    )
                }

                Menu {
                    Button("All Conditions") { viewModel.selectedCondition = nil }
                    Divider()
                    ForEach(ConditionCode.allCases) { code in
                        Button {
                            viewModel.selectedCondition = code
                        } label: {
                            Text("\(code.shortLabel) — \(code.rawValue)")
                        }
                    }
                } label: {
                    FilterChip(
                        label: viewModel.selectedCondition.map { "\($0.shortLabel) — \($0.rawValue)" } ?? "Condition",
                        isActive: viewModel.selectedCondition != nil,
                        sfSymbol: "circle.fill"
                    )
                }

                if !viewModel.uniqueUnits.isEmpty {
                    Menu {
                        Button("All Units") { viewModel.selectedUnit = nil }
                        Divider()
                        ForEach(viewModel.uniqueUnits, id: \.self) { unit in
                            Button(unit) { viewModel.selectedUnit = unit }
                        }
                    } label: {
                        FilterChip(
                            label: viewModel.selectedUnit ?? "Unit",
                            isActive: viewModel.selectedUnit != nil,
                            sfSymbol: "person.3.fill"
                        )
                    }
                }
            }
            .padding(.horizontal)
        }
    }
}

struct FilterChip: View {
    let label: String
    let isActive: Bool
    let sfSymbol: String

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: sfSymbol)
                .font(.caption2)
            Text(label)
                .font(.caption)
                .lineLimit(1)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(isActive ? MilitaryTheme.odGreen : Color(.systemGray5))
        .foregroundStyle(isActive ? .white : .primary)
        .clipShape(Capsule())
    }
}
