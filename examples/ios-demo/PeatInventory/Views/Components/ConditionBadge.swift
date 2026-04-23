import SwiftUI

struct ConditionBadge: View {
    let code: ConditionCode

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(code.color)
                .frame(width: 8, height: 8)
            Text(code.shortLabel)
                .font(.caption.bold())
                .foregroundStyle(code.color)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(code.color.opacity(0.15))
        .clipShape(Capsule())
    }
}

struct ConditionBadgeLarge: View {
    let code: ConditionCode

    var body: some View {
        VStack(spacing: 2) {
            Circle()
                .fill(code.color)
                .frame(width: 12, height: 12)
            Text(code.shortLabel)
                .font(.caption2.bold())
                .foregroundStyle(code.color)
        }
        .frame(width: 36, height: 36)
        .background(code.color.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }
}
