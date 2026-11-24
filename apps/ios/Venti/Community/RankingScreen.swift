import SwiftUI
import VentiCore

/// The pilots of the pilot's Venti server by the points of their best
/// flights that they share with everyone, in a year or in all years.
///
/// The rule, the seasons, the months and every word of a row are the
/// core's and the server's. Under the years stand the months of the year
/// shown that have a ranking, after the year itself, which is the whole
/// of it; over the pilots stands what the ranking is of. Under each pilot
/// stand the flights that count, each of which opens like any public
/// flight.
struct RankingList: View {
    @Environment(CommunityModel.self) private var community

    /// The season shown; choosing another asks the server for it.
    private var season: Binding<String> {
        Binding {
            community.ranked.season
        } set: { season in
            Task {
                await community.rank(season: season)
            }
        }
    }

    /// The month shown, as the core writes it; the empty one is the
    /// whole year. Choosing another asks the server for it.
    private var month: Binding<String> {
        Binding {
            community.ranked.month ?? ""
        } set: { month in
            Task {
                await community.rank(month: month.isEmpty ? nil : month)
            }
        }
    }

    var body: some View {
        let ranked = community.ranked
        VentiList {
            if !ranked.seasons.isEmpty {
                Section {
                    VentiSegmented(
                        options: ranked.seasons.map { (value: $0.name, name: $0.name) },
                        selection: season
                    )
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                    .accessibilityIdentifier("ranking-season")
                    if !ranked.months.isEmpty {
                        VentiSegmented(
                            options: [(value: "", name: ranked.wholeName)]
                                + ranked.months.map { (value: $0.month, name: $0.name) },
                            selection: month
                        )
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: VentiSpace.s2, leading: 0, bottom: 0, trailing: 0))
                        .listRowSeparator(.hidden)
                        .accessibilityIdentifier("ranking-month")
                    }
                    // What the flights ranked are searched by, out of the
                    // way: the season and the month stay as it changes.
                    HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s3) {
                        FlightSearchLine(search: ranked.search)
                        Spacer(minLength: 0)
                        FlightSearchButton(search: ranked.search) { search in
                            Task {
                                await community.rank(search: search)
                            }
                        }
                    }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: VentiSpace.s1, leading: VentiSpace.s1, bottom: 0, trailing: 0))
                    .listRowSeparator(.hidden)
                } footer: {
                    if !ranked.title.isEmpty {
                        Text(ranked.title)
                            .font(.venti(.headline))
                            .foregroundStyle(.venti(.text))
                            .textCase(nil)
                            .padding(.top, VentiSpace.s2)
                            .accessibilityAddTraits(.isHeader)
                            .accessibilityIdentifier("ranking-title")
                    }
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
                    description: Text(
                        ranked.problem ?? (
                            ranked.search.isSearching
                                ? "Nobody shares a flight that matches these choices. Remove one, or widen the area."
                                : ranked.month == nil ? Self.nobody(in: ranked.season) : pilotsWords().monthEmpty
                        )
                    )
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
            PilotFace(name: pilot.name, picturePath: pilot.picturePath, size: VentiSpace.s6)
                .alignmentGuide(.firstTextBaseline) { $0[.bottom] * 0.7 }
            VStack(alignment: .leading, spacing: 2) {
                // The flag of the pilot's country beside the name, once
                // set.
                Text([Countries.flag(pilot.country), pilot.name].compactMap { $0 }.joined(separator: " "))
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
        .accessibilityLabel(
            "Place \(Format.count(pilot.place)), \(pilot.name)" + (pilot.country.map { ", \(Countries.name($0))" } ?? "")
        )
        .accessibilityValue("\(Format.decimal(pilot.points, places: 2)) points, \(pilot.counted)")
    }
}

/// One of the flights that count for a pilot: its day, its distance and
/// its points.
private struct RankedFlightLine: View {
    let flight: PublicFlight

    var body: some View {
        HStack(spacing: 8) {
            Text(Format.day(unixSeconds: flight.startUnixS, zone: flight.timeZone))
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
