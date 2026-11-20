import SwiftUI
import VentiCore

/// The pilots of the pilot's Venti server by the points of their best
/// flights that they share with everyone, in a year or in all years.
///
/// The rule, the seasons and every word of a row are the core's and the
/// server's. Under each pilot stand the flights that count, each of which
/// opens like any public flight.
struct RankingList: View {
    @Environment(CommunityModel.self) private var community

    var body: some View {
        let ranked = community.ranked
        VentiList {
            if !ranked.seasons.isEmpty {
                Section {
                    Picker(
                        "Season",
                        selection: Binding(
                            get: { ranked.season },
                            set: { season in
                                Task {
                                    await community.rank(season: season)
                                }
                            }
                        )
                    ) {
                        ForEach(ranked.seasons, id: \.name) { season in
                            Text(season.name).tag(season.name)
                        }
                    }
                    .pickerStyle(.segmented)
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                    .accessibilityIdentifier("ranking-season")
                }
            }
            ForEach(ranked.pilots, id: \.handle) { pilot in
                Section {
                    NavigationLink {
                        PilotFlightsScreen(pilot: PilotRoute(handle: pilot.handle, name: pilot.name))
                    } label: {
                        RankedPilotHead(pilot: pilot)
                    }
                    .accessibilityIdentifier("ranked-pilot")
                    ForEach(pilot.flights, id: \.id) { flight in
                        NavigationLink(value: flight) {
                            RankedFlightLine(flight: flight)
                        }
                        .accessibilityIdentifier("ranked-flight")
                    }
                }
            }
            if !ranked.pilots.isEmpty {
                Section {
                    EmptyView()
                } footer: {
                    Text(ranked.rule)
                }
            }
            if ranked.isLoading, ranked.pilots.isEmpty {
                ProgressView()
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
            }
        }
        .listStyle(.insetGrouped)
        .overlay {
            if ranked.pilots.isEmpty, !ranked.isLoading {
                ContentUnavailableView(
                    ranked.problem == nil ? "Nobody is ranked" : "The ranking could not be fetched",
                    systemImage: ranked.problem == nil ? "trophy" : "exclamationmark.triangle",
                    description: Text(ranked.problem ?? Self.nobody(in: ranked.season))
                )
            }
        }
        .refreshable {
            await community.rank()
        }
        .task {
            await community.rank()
        }
    }

    /// Why a season has no pilots, and what would change that.
    static func nobody(in season: String) -> String {
        if !season.isEmpty, season.allSatisfy(\.isNumber) {
            return "Nobody shares a flight of \(season) that scores. Choose another year, or all years."
        }
        return "A pilot is ranked once one of their flights is shared with everyone and long enough to score."
    }
}

/// A pilot's place: the place, the name, what counts, and the points.
private struct RankedPilotHead: View {
    let pilot: RankedPilot

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(Format.count(pilot.place))
                .font(.venti(.title2, .bold))
                .monospacedDigit()
                // The first three are the ones looked for.
                .foregroundStyle(pilot.place <= 3 ? Palette.accent : Color.venti(.textSecondary))
                .frame(minWidth: 26)
            VStack(alignment: .leading, spacing: 2) {
                Text(pilot.name)
                    .font(.venti(.headline))
                    .lineLimit(1)
                Text(pilot.counted)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text(Format.decimal(pilot.points, places: 2))
                    .font(.venti(.title3, .bold))
                    .monospacedDigit()
                Text("points")
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Place \(Format.count(pilot.place)), \(pilot.name)")
        .accessibilityValue("\(Format.decimal(pilot.points, places: 2)) points, \(pilot.counted)")
    }
}

/// One of the flights that count for a pilot: its day, its distance and
/// its points.
private struct RankedFlightLine: View {
    let flight: PublicFlight

    var body: some View {
        HStack(spacing: 8) {
            Text(Format.day(unixSeconds: flight.startUnixS))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer(minLength: 8)
            if let score = flight.score, score.kind != .freeFlight {
                Image(systemName: "triangle")
                    .font(.venti(.caption, .semibold))
                    .accessibilityLabel(score.kind == .faiTriangle ? "FAI triangle" : "Triangle")
            }
            Text("\(Format.kilometres(metres: flight.score?.distanceM ?? flight.trackLengthM)) km")
            Text(flight.score.map { Format.decimal($0.points, places: 2) } ?? "–")
                .frame(minWidth: 52, alignment: .trailing)
        }
        .font(.venti(.subheadline))
        .monospacedDigit()
        .accessibilityElement(children: .combine)
    }
}
