import SwiftUI
import VentiCore

/// The card at the map's lower edge while the forecast map is on: the
/// days as the pack's segmented control, the key, the model and when its
/// run began. While the map is asked for, and when there is none, it says
/// so in place of the key. The words of the key, the days and the notes
/// are the core's.
struct ForecastCard: View {
    let state: ThermalMapModel.State
    let map: ThermalMap?
    /// The day on the map.
    let day: ThermalMapDay?
    /// Whether a square of that day lies in the part of the map in view.
    let isInView: Bool
    /// The date of the day on the map.
    @Binding var date: String
    let tryAgain: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            if let map, let day {
                days(of: map, showing: day)
                if isInView {
                    key(map.legend)
                } else {
                    Text("The forecast map has no squares for this part of the map.")
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.textSecondary))
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("forecast-none-here")
                }
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    // Each day is one model's: the days after ICON-D2's are ICON-EU's.
                    Text("\(day.model), run of \(Format.dayAndTime(unixSeconds: day.runUnixS))")
                    if !day.note.isEmpty {
                        Text(day.note)
                    }
                }
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("forecast-model")
            } else if case .failed(let words) = state {
                problem(words)
            } else if map != nil {
                // A map without a day from today on.
                problem("The forecast map has no day from today on.")
            } else {
                HStack(spacing: VentiSpace.s2) {
                    ProgressView()
                    Text("Asking Venti for the forecast map.")
                        .font(.ventiType(.label))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("forecast-asking")
            }
        }
        .padding(VentiSpace.s3)
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiFloating(in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("forecast-card")
    }

    /// The days the map holds, or the one day's name where it holds one.
    @ViewBuilder private func days(of map: ThermalMap, showing day: ThermalMapDay) -> some View {
        if map.days.count > 1 {
            VentiSegmented(options: map.days.map { (value: $0.date, name: $0.name) }, selection: $date)
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Day")
                .accessibilityIdentifier("forecast-days")
        } else {
            Text(day.name)
                .font(.ventiType(.label))
                .accessibilityIdentifier("forecast-days")
        }
    }

    /// The key, the core's: a swatch and the words for each kind of day
    /// from the best down, then "Not flyable". In two columns where they
    /// fit, in one where the text is large or the phone narrow.
    private func key(_ legend: [ThermalLegendEntry]) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: VentiSpace.s4) {
                keyColumn(Array(legend.prefix(3)))
                keyColumn(Array(legend.dropFirst(3)))
            }
            keyColumn(legend)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Key")
        .accessibilityIdentifier("forecast-key")
    }

    private func keyColumn(_ entries: [ThermalLegendEntry]) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            ForEach(entries, id: \.words) { entry in
                HStack(alignment: .top, spacing: VentiSpace.s2) {
                    Swatch(step: entry.scaleStep)
                        // Beside the first line's letters, not above them.
                        .padding(.top, 3)
                    Text(entry.words)
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.text))
                        .fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
            }
        }
    }

    /// What went wrong, in the core's words where it has them, and the one
    /// thing to do about it.
    private func problem(_ words: String) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            HStack(alignment: .top, spacing: VentiSpace.s2) {
                Image("icon-caution")
                    .resizable()
                    .frame(width: 18, height: 18)
                    .foregroundStyle(.venti(.caution))
                    .accessibilityHidden(true)
                Text(words)
                    .font(.ventiType(.label))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("forecast-problem")
            Button("Try again", action: tryAgain)
                .buttonStyle(.venti(.quiet))
        }
    }
}

/// A square of the key in the colour the map fills with: the step of the
/// thermal scale, or the veil of a day that is not flyable. The
/// map's colours, which are the light theme's in both appearances, with
/// an edge so that the palest step stands out on any surface.
private struct Swatch: View {
    let step: UInt8?

    var body: some View {
        // The pack's scale legend (`.v-scale`) has its swatches 12 high,
        // square-cornered.
        Rectangle()
            .fill(step.map { ThermalBand(scaleStep: $0).colorOnMap } ?? Color(uiColor: Palette.notFlyableOnMap))
            .overlay {
                Rectangle()
                    .strokeBorder(.venti(.borderStrong), lineWidth: 1)
            }
            .frame(width: 16, height: 12)
            .accessibilityHidden(true)
    }
}

/// A square of the forecast map that the pilot tapped, for its sheet.
struct ChosenSquare: Identifiable {
    let dayName: String
    let cell: ThermalMapCell
    /// What the map is and is not.
    let note: String
    let credits: [Credit]
    let id: String

    /// The middle of the square: the mean of its edges.
    var middle: Coordinate {
        Coordinate(lat: (cell.south + cell.north) / 2, lon: (cell.west + cell.east) / 2)
    }
}

/// A square of the forecast map, as the pack's sheet: the day; the status
/// as sign, word and colour, with what speaks against the day; the kind of day with its line; the height it
/// was judged at; the way to the forecast for its place; what the map is
/// and whose data it is. The words are the core's.
struct ForecastSquareSheet: View {
    let square: ChosenSquare
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: VentiSpace.s3) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(square.dayName)
                            .font(.ventiType(.heading))
                            .accessibilityAddTraits(.isHeader)
                        Spacer(minLength: VentiSpace.s2)
                        // The status, and under it what speaks against the
                        // day where something does: read one after the other.
                        VStack(alignment: .trailing, spacing: VentiSpace.s1) {
                            VentiStatus(square.cell)
                                .accessibilityIdentifier("forecast-square-status")
                            if !square.cell.why.isEmpty {
                                Text(square.cell.why)
                                    .font(.ventiType(.label))
                                    .foregroundStyle(.venti(.textSecondary))
                                    .multilineTextAlignment(.trailing)
                                    .fixedSize(horizontal: false, vertical: true)
                                    .accessibilityIdentifier("forecast-square-why")
                            }
                        }
                    }
                    if let step = square.cell.scaleStep, !square.cell.thermalWords.isEmpty {
                        ThermalLine(
                            words: square.cell.thermalWords, line: square.cell.thermalLine,
                            band: ThermalBand(scaleStep: step)
                        )
                    }
                    if let ground = square.cell.groundM {
                        Text("Judged at the model's ground, \(Format.metres(ground)) m")
                            .foregroundStyle(.venti(.textSecondary))
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    // The forecast for the middle of the square, as a
                    // site's page leads to it.
                    NavigationLink {
                        ForecastScreen(
                            place: square.middle, title: Format.place(lat: square.middle.lat, lon: square.middle.lon)
                        )
                    } label: {
                        Label("Forecast for this place", systemImage: "cloud.sun")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.quiet))
                    .accessibilityIdentifier("forecast-square-forecast")
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        Text(square.note)
                        ForEach(square.credits, id: \.source) { credit in
                            Text(credit.text)
                        }
                    }
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, VentiSpace.s2)
                }
                .padding(VentiSpace.s4)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("forecast-square")
            }
            .background(.venti(.bg))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }
}

extension VentiStatus {
    /// The pack's status for what the forecast map says of a square.
    init(_ cell: ThermalMapCell) {
        let kind: Kind =
            switch cell.verdict {
            case .flyable: .good
            case .marginal: .caution
            case .notFlyable: .danger
            }
        self.init(kind: kind, word: cell.status)
    }
}
