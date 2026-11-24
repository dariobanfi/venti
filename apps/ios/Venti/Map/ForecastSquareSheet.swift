import SwiftUI
import VentiCore

/// A square of the forecast map that the pilot tapped, for its sheet.
struct ChosenSquare: Identifiable {
    let dayName: String
    let cell: ThermalMapCell
    /// What the map is and is not.
    let note: String
    let credits: [Credit]
    /// The day of the map the square was tapped on, and its model: where
    /// the square's forecast opens.
    let opening: ForecastOpening
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
                    // site's page leads to it, on the day the map showed:
                    // the hours behind that day's verdict.
                    NavigationLink {
                        ForecastScreen(
                            place: square.middle, title: Format.place(lat: square.middle.lat, lon: square.middle.lon),
                            opening: square.opening
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
                    Button(commonWords().done) {
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
