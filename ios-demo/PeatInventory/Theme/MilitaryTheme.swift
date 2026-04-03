import SwiftUI

enum MilitaryTheme {
    static let odGreen = Color(hex: 0x4A5D23)
    static let tan = Color(hex: 0xC2B280)
    static let charcoal = Color(hex: 0x2D2D2D)
    static let alertRed = Color(hex: 0xC0392B)
    static let statusAmber = Color(hex: 0xF39C12)
    static let statusGreen = Color(hex: 0x27AE60)

    static let background = Color(hex: 0x1A1A1A)
    static let cardBackground = Color(hex: 0x2D2D2D)
    static let secondaryText = Color(hex: 0x9E9E9E)
}

extension Color {
    init(hex: UInt, alpha: Double = 1.0) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255.0,
            green: Double((hex >> 8) & 0xFF) / 255.0,
            blue: Double(hex & 0xFF) / 255.0,
            opacity: alpha
        )
    }
}

extension ConditionCode {
    var color: Color {
        switch self {
        case .A: MilitaryTheme.statusGreen
        case .B: Color(hex: 0x2ECC71)
        case .C: MilitaryTheme.statusAmber
        case .D: Color(hex: 0xE67E22)
        case .F: MilitaryTheme.alertRed
        case .H: Color(hex: 0x8E44AD)
        }
    }
}

extension SyncStatus {
    var color: Color {
        switch self {
        case .synced: MilitaryTheme.statusGreen
        case .syncing: MilitaryTheme.odGreen
        case .pendingChanges: MilitaryTheme.statusAmber
        case .offline: MilitaryTheme.secondaryText
        case .error: MilitaryTheme.alertRed
        }
    }
}

struct MilitaryButtonStyle: ButtonStyle {
    var color: Color = MilitaryTheme.odGreen

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
            .background(configuration.isPressed ? color.opacity(0.7) : color)
            .foregroundStyle(.white)
            .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}
