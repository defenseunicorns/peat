import SwiftUI

struct InventoryListView: View {
    @Bindable var viewModel: InventoryViewModel
    @State private var showingAddItem = false
    @State private var editingItem: InventoryItem?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                FilterChipsView(viewModel: viewModel)
                    .padding(.vertical, 8)

                if viewModel.isLoading {
                    Spacer()
                    ProgressView("Loading inventory...")
                    Spacer()
                } else if viewModel.filteredItems.isEmpty {
                    emptyState
                } else {
                    itemList
                }
            }
            .navigationTitle("Inventory")
            .searchable(text: $viewModel.searchText, prompt: "Search NSN, nomenclature, serial...")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button {
                        showingAddItem = true
                    } label: {
                        Image(systemName: "plus.circle.fill")
                            .foregroundStyle(MilitaryTheme.odGreen)
                    }
                }
                ToolbarItem(placement: .topBarLeading) {
                    Text("\(viewModel.filteredItems.count) items")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .sheet(isPresented: $showingAddItem) {
                ItemEditView(viewModel: viewModel, item: nil)
            }
            .sheet(item: $editingItem) { item in
                ItemEditView(viewModel: viewModel, item: item)
            }
        }
    }

    private var itemList: some View {
        List {
            ForEach(viewModel.filteredItems) { item in
                NavigationLink {
                    ItemDetailView(viewModel: viewModel, item: item)
                } label: {
                    InventoryRowView(item: item)
                }
                .swipeActions(edge: .trailing) {
                    Button(role: .destructive) {
                        Task { await viewModel.deleteItem(item) }
                    } label: {
                        Label("Delete", systemImage: "trash")
                    }
                    Button {
                        editingItem = item
                    } label: {
                        Label("Edit", systemImage: "pencil")
                    }
                    .tint(MilitaryTheme.odGreen)
                }
            }
        }
        .listStyle(.plain)
    }

    private var emptyState: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "shippingbox")
                .font(.system(size: 48))
                .foregroundStyle(MilitaryTheme.secondaryText)
            Text(viewModel.hasActiveFilters ? "No items match filters" : "No inventory items")
                .font(.headline)
                .foregroundStyle(.secondary)
            if viewModel.hasActiveFilters {
                Button("Clear Filters") {
                    viewModel.clearFilters()
                }
                .buttonStyle(MilitaryButtonStyle())
            } else {
                Button("Add First Item") {
                    showingAddItem = true
                }
                .buttonStyle(MilitaryButtonStyle())
            }
            Spacer()
        }
    }
}

struct InventoryRowView: View {
    let item: InventoryItem

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: item.category.sfSymbol)
                .font(.title3)
                .foregroundStyle(MilitaryTheme.odGreen)
                .frame(width: 32)

            VStack(alignment: .leading, spacing: 3) {
                Text(item.nomenclature)
                    .font(.subheadline.bold())
                    .lineLimit(1)
                Text(item.nsn)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    Text(item.responsiblePerson)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if item.quantity > 1 {
                        Text("Qty: \(item.quantity)")
                            .font(.caption.bold())
                            .foregroundStyle(MilitaryTheme.odGreen)
                    }
                }
            }

            Spacer()
            ConditionBadge(code: item.conditionCode)
        }
        .padding(.vertical, 2)
    }
}
