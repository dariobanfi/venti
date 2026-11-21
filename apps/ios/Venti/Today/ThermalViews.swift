import SwiftUI
import VentiCore

/// A mark in the colour of a forecast climb, from the design language's
/// thermal scale. The number or the words stand beside it, as the pack
/// asks of every scale: the colour alone says nothing.
struct ThermalMark: View {
    let band: ThermalBand

    init(climbMs: Double) {
        band = ThermalBand(metresPerSecond: climbMs)
    }

    /// A mark in a step the core named, as for a square of the thermal map.
    init(band: ThermalBand) {
        self.band = band
    }

    var body: some View {
        Circle()
            .fill(band.color)
            .overlay {
                // The scale's palest step is close to the surface it lies on.
                Circle()
                    .strokeBorder(.venti(.borderStrong), lineWidth: 1)
            }
            .frame(width: 12, height: 12)
            .accessibilityHidden(true)
    }
}

/// The thermals of a day: the kind of day beside its mark, and under it
/// the figures in a line. Both are the core's estimate from the weather
/// model, by fixed limits: for a take-off, or for a square of the map.
struct ThermalLine: View {
    let words: String
    let line: String
    let band: ThermalBand

    /// The thermals of a day at a take-off.
    init(day: OutlookDay) {
        self.init(words: day.thermalWords, line: day.thermalLine, band: ThermalBand(metresPerSecond: day.climbMs ?? 0))
    }

    init(words: String, line: String, band: ThermalBand) {
        self.words = words
        self.line = line
        self.band = band
    }

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s2) {
            ThermalMark(band: band)
                // Beside the first line's letters, not above them.
                .padding(.top, 3)
            VStack(alignment: .leading, spacing: 2) {
                Text(words)
                    .font(.ventiType(.label))
                // A kind of day without its figures has no line.
                if !line.isEmpty {
                    Text(line)
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.textSecondary))
                        // On as many lines as it needs, also inside a card
                        // that is a button, where a line is cut short.
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("thermals")
    }
}
