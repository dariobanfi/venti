import SwiftUI
import VentiCore

/// The wind stations around the part of the map in view, as a list.
struct StationList: View {
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    @State private var search = ""

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    ForEach(shown, id: \.id) { station in
                        NavigationLink(value: station) {
                            StationRow(station: station, now: wind.now)
                        }
                        .accessibilityIdentifier("station-row")
                    }
                } footer: {
                    if !wind.credits.isEmpty {
                        Text(wind.credits.map(\.text).joined(separator: "\n"))
                    }
                }
            }
            .overlay {
                if wind.stations.isEmpty {
                    ContentUnavailableView(
                        "No stations here",
                        systemImage: "wind",
                        description: Text(wind.problem ?? "Move the map to an area with wind stations.")
                    )
                } else if shown.isEmpty {
                    ContentUnavailableView.search(text: search)
                }
            }
            .searchable(text: $search, prompt: "Station")
            .navigationTitle("Wind stations")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: WindStation.self) { station in
                StationScreen(station: station)
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    /// By name, the way a pilot looks one up.
    private var shown: [WindStation] {
        let wanted = search.trimmingCharacters(in: .whitespaces)
        return wind.stations
            .filter { wanted.isEmpty || $0.name.localizedCaseInsensitiveContains(wanted) }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }
}

/// One station in a line: where the wind blows, its name and height, and
/// the wind in numbers.
struct StationRow: View {
    let station: WindStation
    /// The current time by the server's clock.
    let now: Int64

    var body: some View {
        let mark = station.mark
        HStack(spacing: 12) {
            WindGlyph(mark: mark)
                .frame(width: 28, height: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(station.name)
                    .lineLimit(1)
                Text(detail)
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            if !mark.label.isEmpty {
                Text("\(Text(mark.label).font(.venti(.body, .semibold)))\(Text(" km/h").font(.venti(.footnote)).foregroundStyle(.secondary))")
                    .monospacedDigit()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(station.name)
        .accessibilityValue(spoken(mark))
    }

    /// The station's height, and how old its reading is if that matters.
    private var detail: String {
        var parts: [String] = []
        if let elevation = station.elevationM {
            parts.append("\(Format.metres(elevation)) m")
        }
        if let reading = station.latest {
            if station.isOld {
                parts.append(Format.age(minutes: Format.minutes(from: reading.timeUnixS, to: now)))
            }
        } else {
            parts.append("no reading")
        }
        return parts.joined(separator: " · ")
    }

    private func spoken(_ mark: WindMark) -> String {
        guard let reading = station.latest, let speed = reading.speedMs, !station.isOld else {
            return "No current reading. \(detail)"
        }
        var parts = ["Wind \(Format.spokenKilometresPerHour(metresPerSecond: speed))"]
        if let gust = reading.gustMs {
            parts.append("gusts \(Format.kilometresPerHour(metresPerSecond: gust))")
        }
        if case .arrow = mark.symbol, let from = reading.directionDeg {
            parts.append("from \(Format.compassPoint(degrees: from))")
        }
        return parts.joined(separator: ", ")
    }
}

/// The map's symbol for a station, drawn for a list or a heading.
struct WindGlyph: View {
    let mark: WindMark

    var body: some View {
        switch mark.symbol {
        case .arrow(let band):
            Image(systemName: "location.north.fill")
                .font(.system(size: 20))
                .foregroundStyle(band.ink)
                .rotationEffect(.degrees(mark.headingDeg))
        case .calm:
            Image(systemName: "circle.fill")
                .font(.system(size: 9))
                .foregroundStyle(WindBand.light.ink)
        case .old:
            Image(systemName: "circle.fill")
                .font(.system(size: 9))
                .foregroundStyle(.tertiary)
        }
    }
}
