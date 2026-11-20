import SwiftUI

/// One number with its unit and what it measures: the building block of the
/// logbook totals and the flight panel.
///
/// Numbers are set in the rounded system face with fixed-width digits, so a
/// value that changes while the pilot drags along the chart does not jitter.
struct Metric: View {
    let caption: String
    let value: String
    var unit: String = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text("\(valueText)\(unitText)")
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            Text(caption)
                .font(.venti(.caption))
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(caption)
        .accessibilityValue(unit.isEmpty ? value : "\(value) \(unit)")
    }

    private var valueText: Text {
        Text(value).font(.venti(.title3, .semibold))
    }

    private var unitText: Text {
        Text(unit.isEmpty ? "" : " \(unit)")
            .font(.venti(.footnote, .medium))
            .foregroundStyle(.secondary)
    }
}

#Preview {
    HStack {
        Metric(caption: "Airtime", value: "0:29", unit: "h")
        Metric(caption: "Highest", value: "2,117", unit: "m")
        Metric(caption: "Best climb", value: "+2.4", unit: "m/s")
    }
    .padding()
}
