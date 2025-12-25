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

    init() {
        #if os(macOS)
        // Ensure app activates properly when run from command line
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.activate(ignoringOtherApps: true)
        #endif
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
                #if os(macOS)
                .frame(minWidth: 380, minHeight: 500)
                #endif
        }
        #if os(macOS)
        .defaultSize(width: 400, height: 600)
        .commands {
            // Add standard menu commands
            CommandGroup(replacing: .appInfo) {
                Button("About HIVE Test") {
                    NSApplication.shared.orderFrontStandardAboutPanel(
                        options: [
                            NSApplication.AboutPanelOptionKey.applicationName: "HIVE Test",
                            NSApplication.AboutPanelOptionKey.applicationVersion: "1.0",
                            NSApplication.AboutPanelOptionKey.credits: NSAttributedString(string: "BLE Mesh Testing App")
                        ]
                    )
                }
            }
        }
        #endif
    }
}

#if os(macOS)
import AppKit
#endif
