//
//  Color+Platform.swift
//  HiveTest
//
//  Cross-platform color helpers
//

import SwiftUI

extension Color {
    /// Primary background color (system background on iOS, window background on macOS)
    static var systemBackground: Color {
        #if os(iOS)
        return Color(uiColor: .systemBackground)
        #else
        return Color(nsColor: .windowBackgroundColor)
        #endif
    }

    /// Secondary system background
    static var secondarySystemBackground: Color {
        #if os(iOS)
        return Color(uiColor: .secondarySystemBackground)
        #else
        return Color(nsColor: .controlBackgroundColor)
        #endif
    }

    /// Tertiary system background
    static var tertiarySystemBackground: Color {
        #if os(iOS)
        return Color(uiColor: .tertiarySystemBackground)
        #else
        return Color(nsColor: .textBackgroundColor)
        #endif
    }
}
