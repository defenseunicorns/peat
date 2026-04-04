import SwiftUI

struct ContentView: View {
    let inventoryVM: InventoryViewModel
    let meshVM: MeshViewModel
    let demoVM: DemoViewModel
    @State private var showingSettings = false
    @State private var selectedTab = 0

    var body: some View {
        TabView(selection: $selectedTab) {
            InventoryListView(viewModel: inventoryVM)
                .tabItem {
                    Label("Inventory", systemImage: "shippingbox.fill")
                }
                .tag(0)

            MeshStatusView(viewModel: meshVM)
                .tabItem {
                    Label("Mesh", systemImage: "circle.hexagongrid.fill")
                }
                .badge(meshVM.connectedPeerCount > 0 ? "\(meshVM.connectedPeerCount)" : nil)
                .tag(1)

            AggregationView(viewModel: inventoryVM)
                .tabItem {
                    Label("Aggregate", systemImage: "chart.bar.fill")
                }
                .tag(2)
        }
        .tint(MilitaryTheme.odGreen)
        .overlay(alignment: .topTrailing) {
            settingsButton
        }
        .sheet(isPresented: $showingSettings) {
            DemoSettingsView(demoVM: demoVM, inventoryVM: inventoryVM)
        }
        .task {
            await inventoryVM.loadItems()
        }
    }

    private var settingsButton: some View {
        Button {
            showingSettings = true
        } label: {
            Image(systemName: "gearshape.fill")
                .font(.body)
                .foregroundStyle(MilitaryTheme.odGreen)
                .padding(10)
                .background(.ultraThinMaterial)
                .clipShape(Circle())
        }
        .padding(.trailing, 16)
        .padding(.top, 4)
    }
}
