import SwiftUI

@main
struct PeatInventoryApp: App {
    @State private var service: PeatServiceProtocol
    @State private var inventoryVM: InventoryViewModel
    @State private var meshVM: MeshViewModel
    @State private var demoVM: DemoViewModel

    init() {
        let svc: PeatServiceProtocol
        if ProcessInfo.processInfo.environment["PEAT_USE_MOCK"] != nil || true {
            // Default to mock for now — switch to PeatServiceLive when FFI is ready
            svc = PeatServiceMock()
        } else {
            svc = PeatServiceMock() // Replace with PeatServiceLive when available
        }

        let invVM = InventoryViewModel(service: svc)
        let meshVM = MeshViewModel(service: svc)
        let demoVM = DemoViewModel(service: svc, inventoryVM: invVM, meshVM: meshVM)

        self._service = State(initialValue: svc)
        self._inventoryVM = State(initialValue: invVM)
        self._meshVM = State(initialValue: meshVM)
        self._demoVM = State(initialValue: demoVM)
    }

    var body: some Scene {
        WindowGroup {
            ContentView(
                inventoryVM: inventoryVM,
                meshVM: meshVM,
                demoVM: demoVM
            )
            .task {
                do {
                    try await service.start()
                } catch {
                    print("Failed to start Peat service: \(error)")
                }
            }
        }
    }
}
