import SwiftUI
import VentiCore

/// A mark in the colour of a forecast climb, from the design language's
/// thermal scale. The number or the words stand beside it, as the pack
/// asks of every scale: the colour alone says nothing.
struct ThermalMark: View {
    let climbMs: Double

    var body: some View {
        Circle()
            .fill(ThermalBand(metresPerSecond: climbMs).color)
            .overlay {
                // The scale's palest step is close to the surface it lies on.
                Circle()
                    .strokeBorder(.venti(.borderStrong), lineWidth: 1)
            }
            .frame(width: 12, height: 12)
            .accessibilityHidden(true)
    }
}

/// The thermals of a day at a take-off: the kind of day beside its mark,
/// and under it the figures in a line. Both are the core's estimate from
/// the weather model, by fixed limits.
struct ThermalLine: View {
    let day: OutlookDay

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s2) {
            ThermalMark(climbMs: day.climbMs ?? 0)
                // Beside the first line's letters, not above them.
                .padding(.top, 3)
            VStack(alignment: .leading, spacing: 2) {
                Text(day.thermalWords)
                    .font(.ventiType(.label))
                Text(day.thermalLine)
                    .font(.ventiType(.label))
                    .foregroundStyle(.venti(.textSecondary))
                    // On as many lines as it needs, also inside a card
                    // that is a button, where a line is cut short.
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("thermals")
    }
}
