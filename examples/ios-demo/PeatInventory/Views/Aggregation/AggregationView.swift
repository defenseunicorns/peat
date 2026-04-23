import SwiftUI

struct AggregationView: View {
    let viewModel: InventoryViewModel
    @State private var selectedCategory: EquipmentCategory?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    summaryCards
                    categoryBreakdown
                    conditionBreakdown
                    if let category = selectedCategory {
                        categoryDetail(category)
                    }
                    unitBreakdown
                }
                .padding()
            }
            .navigationTitle("Aggregation")
        }
    }

    private var summaryCards: some View {
        LazyVGrid(columns: [
            GridItem(.flexible()),
            GridItem(.flexible()),
            GridItem(.flexible()),
        ], spacing: 12) {
            SummaryCard(
                title: "Total Items",
                value: "\(viewModel.items.count)",
                sfSymbol: "shippingbox.fill",
                color: MilitaryTheme.odGreen
            )
            SummaryCard(
                title: "Total Qty",
                value: "\(viewModel.items.reduce(0) { $0 + $1.quantity })",
                sfSymbol: "number",
                color: MilitaryTheme.tan
            )
            SummaryCard(
                title: "Needs Attn",
                value: "\(viewModel.items.filter { $0.conditionCode != .A }.count)",
                sfSymbol: "exclamationmark.triangle.fill",
                color: needsAttentionCount > 0 ? MilitaryTheme.statusAmber : MilitaryTheme.statusGreen
            )
        }
    }

    private var needsAttentionCount: Int {
        viewModel.items.filter { $0.conditionCode != .A }.count
    }

    private var categoryBreakdown: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "By Category", sfSymbol: "square.grid.2x2.fill")

            ForEach(viewModel.itemCountByCategory, id: \.0) { category, count in
                Button {
                    withAnimation {
                        selectedCategory = selectedCategory == category ? nil : category
                    }
                } label: {
                    HStack {
                        Image(systemName: category.sfSymbol)
                            .foregroundStyle(MilitaryTheme.odGreen)
                            .frame(width: 24)
                        Text(category.rawValue)
                            .foregroundStyle(.primary)
                        Spacer()
                        Text("\(count)")
                            .font(.headline.monospaced())
                            .foregroundStyle(MilitaryTheme.odGreen)
                        if selectedCategory == category {
                            Image(systemName: "chevron.down")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        } else {
                            Image(systemName: "chevron.right")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 8)
                    .padding(.horizontal, 12)
                    .background(selectedCategory == category ? MilitaryTheme.odGreen.opacity(0.08) : Color.clear)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }
            }
        }
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }

    private func categoryDetail(_ category: EquipmentCategory) -> some View {
        let categoryItems = viewModel.items.filter { $0.category == category }
        return VStack(alignment: .leading, spacing: 8) {
            Text("\(category.rawValue) — \(categoryItems.count) items")
                .font(.subheadline.bold())
                .foregroundStyle(MilitaryTheme.odGreen)

            ForEach(categoryItems) { item in
                HStack {
                    VStack(alignment: .leading) {
                        Text(item.nomenclature)
                            .font(.caption.bold())
                            .lineLimit(1)
                        Text(item.nsn)
                            .font(.caption2.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text("x\(item.quantity)")
                        .font(.caption.monospaced())
                    ConditionBadge(code: item.conditionCode)
                }
                .padding(.vertical, 2)
            }
        }
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
        .transition(.asymmetric(insertion: .move(edge: .top).combined(with: .opacity), removal: .opacity))
    }

    private var conditionBreakdown: some View {
        VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "By Condition", sfSymbol: "chart.bar.fill")

            ForEach(ConditionCode.allCases) { code in
                let count = viewModel.items.filter { $0.conditionCode == code }.count
                let totalQty = viewModel.items.filter { $0.conditionCode == code }.reduce(0) { $0 + $1.quantity }
                if count > 0 {
                    HStack {
                        ConditionBadgeLarge(code: code)
                        VStack(alignment: .leading) {
                            Text(code.rawValue)
                                .font(.subheadline)
                            Text("\(count) items, \(totalQty) total qty")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        conditionBar(count: count, total: viewModel.items.count, color: code.color)
                    }
                }
            }
        }
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }

    private func conditionBar(count: Int, total: Int, color: Color) -> some View {
        let fraction = total > 0 ? Double(count) / Double(total) : 0
        return HStack(spacing: 4) {
            GeometryReader { geo in
                RoundedRectangle(cornerRadius: 2)
                    .fill(color)
                    .frame(width: geo.size.width * fraction)
            }
            .frame(width: 60, height: 8)
            .background(Color(.systemGray5))
            .clipShape(RoundedRectangle(cornerRadius: 2))

            Text("\(Int(fraction * 100))%")
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .frame(width: 32, alignment: .trailing)
        }
    }

    private var unitBreakdown: some View {
        let unitGroups = Dictionary(grouping: viewModel.items, by: \.responsibleUnit)
        let sortedUnits = unitGroups.sorted { $0.key < $1.key }

        return VStack(alignment: .leading, spacing: 12) {
            SectionHeader(title: "By Unit", sfSymbol: "person.3.fill")

            ForEach(sortedUnits, id: \.key) { unit, items in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(unit)
                            .font(.subheadline.bold())
                        Spacer()
                        Text("\(items.count) items")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    HStack(spacing: 4) {
                        ForEach(EquipmentCategory.allCases) { cat in
                            let catCount = items.filter { $0.category == cat }.count
                            if catCount > 0 {
                                HStack(spacing: 2) {
                                    Image(systemName: cat.sfSymbol)
                                        .font(.caption2)
                                    Text("\(catCount)")
                                        .font(.caption2)
                                }
                                .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                .padding(.vertical, 4)
                if unit != sortedUnits.last?.key {
                    Divider()
                }
            }
        }
        .padding()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }
}

struct SummaryCard: View {
    let title: String
    let value: String
    let sfSymbol: String
    let color: Color

    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: sfSymbol)
                .font(.title3)
                .foregroundStyle(color)
            Text(value)
                .font(.title2.bold().monospaced())
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 16)
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)
    }
}
