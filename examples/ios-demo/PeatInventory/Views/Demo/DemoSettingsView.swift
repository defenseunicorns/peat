import SwiftUI

struct DemoSettingsView: View {
    @Bindable var demoVM: DemoViewModel
    let inventoryVM: InventoryViewModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Toggle("Demo Mode", isOn: Binding(
                        get: { demoVM.isDemoMode },
                        set: { newValue in
                            Task {
                                if newValue {
                                    await demoVM.enableDemoMode()
                                } else {
                                    await demoVM.disableDemoMode()
                                }
                            }
                        }
                    ))
                    .tint(MilitaryTheme.odGreen)

                    if demoVM.isDemoMode {
                        Picker("Node Identity", selection: $demoVM.selectedNodeName) {
                            ForEach(DemoDataService.demoNodeNames, id: \.self) { name in
                                Text(name).tag(name)
                            }
                        }
                        .onChange(of: demoVM.selectedNodeName) { _, newValue in
                            Task { await demoVM.updateNodeName(newValue) }
                        }
                    }
                } header: {
                    Text("Demo Configuration")
                } footer: {
                    Text("Demo mode pre-populates realistic military inventory data and sets a military node identity.")
                }

                if demoVM.isDemoMode {
                    Section("Demo Scenarios") {
                        ForEach(DemoScenario.allCases) { scenario in
                            Button {
                                Task { await demoVM.runScenario(scenario) }
                            } label: {
                                HStack {
                                    Image(systemName: scenario.sfSymbol)
                                        .foregroundStyle(MilitaryTheme.odGreen)
                                        .frame(width: 28)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(scenario.title)
                                            .font(.subheadline.bold())
                                            .foregroundStyle(.primary)
                                        Text(scenario.description)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    if demoVM.selectedScenario == scenario && demoVM.scenarioRunning {
                                        ProgressView()
                                    }
                                }
                            }
                            .disabled(demoVM.scenarioRunning)
                        }
                    }

                    if !demoVM.scenarioLog.isEmpty {
                        Section("Scenario Log") {
                            ForEach(Array(demoVM.scenarioLog.enumerated()), id: \.offset) { _, entry in
                                Text(entry)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }

                Section("Data Management") {
                    Button("Load Demo Data") {
                        Task { await inventoryVM.loadDemoData() }
                    }
                    .foregroundStyle(MilitaryTheme.odGreen)

                    Button("Clear All Data", role: .destructive) {
                        Task { await inventoryVM.clearAllData() }
                    }
                }

                Section("About") {
                    HStack {
                        Text("Peat Protocol Version")
                        Spacer()
                        Text("Demo")
                            .foregroundStyle(.secondary)
                    }
                    HStack {
                        Text("Transport Layer")
                        Spacer()
                        Text("MultipeerConnectivity")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    HStack {
                        Text("CRDT Backend")
                        Spacer()
                        Text("Last-Write-Wins")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
