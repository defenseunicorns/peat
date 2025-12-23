//
//  HiveTestApp.swift
//  HiveTest
//
//  HIVE BLE Test Application for iOS/macOS
//

import SwiftUI

@main
struct HiveTestApp: App {
    @StateObject private var viewModel = HiveViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
        }
        #if os(macOS)
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 400, height: 600)
        #endif
    }
}
