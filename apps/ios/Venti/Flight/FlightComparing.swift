import CoreLocation
import SwiftUI
import VentiCore

/// A second flight laid beside the one on screen.
///
/// Flights that were in the air at the same time are laid over each other
/// by the clock; others each from their first moment. The core says which,
/// and what there is to compare in numbers.
struct ComparedFlight: Equatable {
    let flight: FlightView
    /// Whose it is, or which of the pilot's own: "Mara Muster", "Your
    /// flight of 12 Nov".
    let name: String
    let comparison: FlightComparison

    /// Where the other glider was at a moment of the flight on screen.
    /// `nil` when it was not in the air then.
    func point(atSeconds seconds: UInt32) -> TrackPoint? {
        let theirs = Double(seconds) - Double(comparison.offsetS)
        guard theirs >= 0, theirs <= Double(flight.durationS), !flight.track.isEmpty else { return nil }
        return flight.track[AltitudeChart.nearestIndex(in: flight.track, toSeconds: theirs)]
    }

    /// What to say about the other glider at a moment of the flight on
    /// screen: its altitude and how far away it was. `nil` when it was
    /// not in the air then.
    func words(at point: TrackPoint) -> String? {
        guard let other = self.point(atSeconds: point.seconds) else { return nil }
        let apart = CLLocation(latitude: point.lat, longitude: point.lon)
            .distance(from: CLLocation(latitude: other.lat, longitude: other.lon))
        let away = apart < 1000
            ? "\(Format.metres((apart / 10).rounded() * 10)) m away"
            : "\(Format.kilometres(metres: apart)) km away"
        return "\(Format.metres(other.altitudeM)) m · \(away)"
    }

    /// The position in the track of the flight on screen at which the
    /// two were nearest, if they flew at the same time.
    func nearestIndex(in track: [TrackPoint]) -> Int? {
        comparison.nearest.map { AltitudeChart.nearestIndex(in: track, toSeconds: Double($0.seconds)) }
    }
}

extension Palette {
    /// The colour of a second flight laid beside the one on screen: the
    /// design language's first colour for comparing flights, which no
    /// climb band has. On the map, which is light in both appearances,
    /// the light theme's.
    static var otherFlight: Color {
        Color(uiColor: UIColor { traits in UIColor(VentiTheme.of(traits).series[0]) })
    }

    static var otherFlightOnMap: UIColor { UIColor(VentiTheme.light.series[0]) }
}

/// Chooses the flight to lay beside the one on screen: one of the pilot's
/// own, or one that other pilots show to everyone and that was flown
/// around the same place on the same day.
struct CompareChooser: View {
    let flight: FlightView
    let analysis: FlightAnalysis?
    /// Called with the flight chosen, compared.
    let onChosen: (ComparedFlight) -> Void

    @Environment(LogbookModel.self) private var logbook
    @Environment(CommunityModel.self) private var community
    @Environment(\.dismiss) private var dismiss

    /// What others flew there that day. `nil` until the server answered.
    @State private var others: [PublicFlight]?
    /// Why the list of others' flights is not there.
    @State private var listProblem: String?
    /// Why the flight that was chosen could not be laid beside this one.
    @State private var problem: String?
    /// The row that was tapped, while its flight is fetched and compared.
    @State private var loading: String?

    var body: some View {
        NavigationStack {
            VentiList {
                if community.server() != nil {
                    Section {
                        if let others {
                            if others.isEmpty {
                                Text("Nobody shows a flight from around here on that day.")
                                    .foregroundStyle(.secondary)
                            }
                            ForEach(others, id: \.id) { other in
                                Button {
                                    choose(other)
                                } label: {
                                    PublicFlightRow(flight: other)
                                        .overlay(alignment: .trailing) { spinner(for: other.id) }
                                        .contentShape(.rect)
                                }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier("compare-public-row")
                            }
                        } else if let listProblem {
                            Text(listProblem)
                                .foregroundStyle(.secondary)
                        } else {
                            ProgressView()
                                .frame(maxWidth: .infinity)
                        }
                    } header: {
                        Text("Flown here that day")
                    } footer: {
                        Text("Flights that other pilots show to everyone, begun within twelve hours of this one around where it was flown.")
                    }
                }
                Section("Your flights") {
                    if own.isEmpty {
                        Text("Your logbook holds no other flight.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(own, id: \.id) { entry in
                        Button {
                            choose(entry)
                        } label: {
                            OwnFlightLine(entry: entry)
                                .overlay(alignment: .trailing) { spinner(for: entry.id) }
                                .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("compare-own-row")
                    }
                }
            }
            .disabled(loading != nil)
            .navigationTitle("Lay a flight beside it")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
            }
            .task {
                await loadOthers()
            }
            .alert(
                "This flight could not be laid beside it",
                isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })
            ) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(problem ?? "")
            }
        }
    }

    @ViewBuilder private func spinner(for id: String) -> some View {
        if loading == id {
            ProgressView()
        }
    }

    /// The pilot's other flights, those nearest in time first: flights of
    /// the same day lead the list. No more than fifty.
    private var own: [LogbookEntry] {
        logbook.view.years
            .flatMap(\.flights)
            .filter { $0.id != flight.id && $0.startUnixS != flight.startUnixS }
            .sorted { abs($0.startUnixS - flight.startUnixS) < abs($1.startUnixS - flight.startUnixS) }
            .prefix(50)
            .map { $0 }
    }

    private func loadOthers() async {
        guard community.server() != nil else { return }
        do {
            let found = try await community.flights(
                around: flight.bounds.grown(by: 0.1),
                fromUnixS: flight.startUnixS - 12 * 3600,
                toUnixS: flight.startUnixS + 12 * 3600
            )
            // The flight on screen may be one of them.
            others = found.filter { $0.startUnixS != flight.startUnixS }
        } catch {
            listProblem = CommunityModel.words(for: error)
        }
    }

    private func choose(_ entry: LogbookEntry) {
        guard loading == nil else { return }
        loading = entry.id
        Task {
            defer { loading = nil }
            do {
                let other = try await logbook.flight(id: entry.id)
                let otherAnalysis = try? await logbook.analysis(id: entry.id)
                let name = "Your flight of \(Format.dayShort(unixSeconds: entry.startUnixS))"
                await finish(with: other, otherAnalysis, named: name)
            } catch {
                problem = error.words
            }
        }
    }

    private func choose(_ other: PublicFlight) {
        guard loading == nil else { return }
        loading = other.id
        Task {
            defer { loading = nil }
            do {
                let (view, otherAnalysis) = try await community.open(other)
                await finish(with: view, otherAnalysis, named: other.pilot)
            } catch {
                problem = CommunityModel.words(for: error)
            }
        }
    }

    /// Compares, off the main thread: both tracks go to the core whole.
    private func finish(with other: FlightView, _ otherAnalysis: FlightAnalysis?, named name: String) async {
        let (first, firstAnalysis) = (flight, analysis)
        let comparison = await Task.detached(priority: .userInitiated) {
            compareFlights(first: first, other: other, firstAnalysis: firstAnalysis, otherAnalysis: otherAnalysis)
        }.value
        onChosen(ComparedFlight(flight: other, name: name, comparison: comparison))
        dismiss()
    }
}

/// One of the pilot's own flights, in a line of the chooser.
private struct OwnFlightLine: View {
    let entry: LogbookEntry

    var body: some View {
        HStack(spacing: 12) {
            TrackThumbnail(outline: entry.outline)
            VStack(alignment: .leading, spacing: 2) {
                Text(Format.day(unixSeconds: entry.startUnixS))
                    .font(.venti(.headline))
                    .lineLimit(1)
                Text(entry.takeoffName ?? Format.time(unixSeconds: entry.startUnixS))
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(Format.kilometres(metres: entry.score?.distanceM ?? entry.trackLengthM)) km")
                    .font(.venti(.headline))
                Text("\(Format.hoursMinutes(seconds: entry.durationS)) h")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
            }
            .monospacedDigit()
        }
        .accessibilityElement(children: .combine)
    }
}

/// The two flights in numbers, side by side, with what the pilot can do
/// from here: go to where the two were nearest, and end the comparison.
struct ComparisonSheet: View {
    let compared: ComparedFlight
    /// Called to mark the moment the two were nearest.
    let onNearest: () -> Void
    /// Called to take the other flight away again.
    let onEnd: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                        GridRow {
                            Text("")
                            Text("This flight")
                            Text(compared.name)
                        }
                        .font(.venti(.footnote, .semibold))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        ForEach(compared.comparison.rows, id: \.label) { row in
                            GridRow {
                                Text(row.label)
                                    .foregroundStyle(.secondary)
                                Text(row.first)
                                    .fontWeight(row.lead == .first ? .semibold : .regular)
                                Text(row.other)
                                    .fontWeight(row.lead == .other ? .semibold : .regular)
                                    .foregroundStyle(Palette.otherFlight)
                            }
                            .monospacedDigit()
                            .accessibilityElement(children: .combine)
                        }
                    }
                    .font(.venti(.subheadline))
                    .accessibilityIdentifier("comparison-rows")
                } header: {
                    Text(compared.comparison.words)
                        .textCase(nil)
                } footer: {
                    Text("The number that is ahead is set in bold. Worked out by Venti from the two recordings; where one of them is cut short, so are its numbers.")
                }

                Section {
                    if compared.comparison.nearest != nil {
                        Button("Go to where they were nearest", systemImage: "scope") {
                            onNearest()
                            dismiss()
                        }
                    }
                    Button("Take the other flight away", systemImage: "xmark.circle", role: .destructive) {
                        onEnd()
                        dismiss()
                    }
                    .accessibilityIdentifier("compare-end")
                }
            }
            .navigationTitle("Side by side")
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
