import SwiftUI

struct TransportBadge: View {
    let transport: TransportType
    var isActive: Bool = true

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: transport.sfSymbol)
                .font(.caption2)
            Text(transport.rawValue)
                .font(.caption2)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(isActive ? MilitaryTheme.odGreen.opacity(0.2) : Color(.systemGray5))
        .foregroundStyle(isActive ? MilitaryTheme.odGreen : .secondary)
        .clipShape(Capsule())
    }
}

struct SyncStatusBadge: View {
    let status: SyncStatus

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: status.sfSymbol)
                .font(.caption)
                .foregroundStyle(status.color)
                .symbolEffect(.pulse, isActive: status == .syncing)
            Text(status.rawValue)
                .font(.caption.bold())
                .foregroundStyle(status.color)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(status.color.opacity(0.12))
        .clipShape(Capsule())
    }
}

struct SignalBars: View {
    let strength: SignalStrength

    var body: some View {
        HStack(spacing: 1) {
            ForEach(0..<3, id: \.self) { i in
                RoundedRectangle(cornerRadius: 1)
                    .fill(i < strength.bars ? MilitaryTheme.statusGreen : Color(.systemGray4))
                    .frame(width: 3, height: CGFloat(6 + i * 3))
            }
        }
        .frame(height: 12, alignment: .bottom)
    }
}
