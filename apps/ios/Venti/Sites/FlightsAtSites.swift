import Foundation
import Observation
import SwiftUI
import VentiCore

// What public flights say about where pilots take off: the flights that
// began at a site, on its page; the places where several pilots took off
// and no site stands, as a layer of the map; and the offer to add such a
// place to a pilot who took off there. What is counted, every word and
// who is offered what are the core's (`takeoffs.rs`); this asks the
// server, draws and holds no rule.

/// The core's words around both.
enum FlightsAtSitesWords {
    static let core = siteFlightsWords()
}

// MARK: A site's flights

/// What the public flights that began at a site say about it, read from
/// Venti without a token.
enum SiteFlightsReading {
    /// Asks Venti for a site's flights and has the core read the answer.
    static func read(
        _ id: String, from server: URL, transport: @Sendable (URL) async throws -> (Int, Data) = WindModel.fetch
    ) async throws -> SiteFlights {
        let path = try siteFlightsPath(id: id)
        guard let url = URL(string: server.absoluteString + path) else { throw URLError(.badURL) }
        let (status, body) = try await transport(url)
        return try siteFlights(status: UInt16(clamping: status), body: body)
    }
}

/// The section of a site's page about the public flights that began
/// there, under the confirmations: the count, what is counted, the twelve
/// months as bars with the season under them, and the best flights, each
/// opening as a public flight opens. Nothing at all for a site that is
/// not a take-off.
struct SiteFlightsSection: View {
    let flights: SiteFlights

    var body: some View {
        let words = FlightsAtSitesWords.core
        if flights.isTakeoff {
            Section {
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    Text(flights.words.isEmpty ? flights.nobodyWords : flights.words)
                        .accessibilityIdentifier("site-flights-words")
                    if !flights.about.isEmpty {
                        Text(flights.about)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("site-flights-about")
                    }
                }
                if flights.flights > 0, !flights.months.isEmpty {
                    VStack(alignment: .leading, spacing: VentiSpace.s2) {
                        Text(words.monthsTitle)
                            .font(.ventiType(.label))
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityAddTraits(.isHeader)
                        MonthBars(months: flights.months)
                        if !flights.seasonWords.isEmpty {
                            Text(flights.seasonWords)
                                .accessibilityIdentifier("site-flights-season")
                        }
                    }
                    .padding(.vertical, VentiSpace.s1)
                }
            } header: {
                Text(words.title)
                    .textCase(nil)
                    .accessibilityIdentifier("site-flights")
            } footer: {
                if flights.best.isEmpty, !flights.terms.text.isEmpty {
                    Text(flights.terms.text)
                }
            }
            if !flights.best.isEmpty {
                Section {
                    ForEach(flights.best, id: \.id) { line in
                        NavigationLink {
                            PublicFlightScreen(flight: PublicFlight(best: line))
                        } label: {
                            Text(line.words)
                        }
                        .accessibilityIdentifier("site-best-flight")
                    }
                } header: {
                    Text(words.bestTitle)
                        .textCase(nil)
                } footer: {
                    if !flights.terms.text.isEmpty {
                        Text(flights.terms.text)
                    }
                }
            }
        }
    }
}

/// The twelve months as a small row of bars, January first: each bar as
/// tall as the core's share, the month's short name under it, and the
/// core's words for a reader of the screen. In the accent, as the
/// logbook's months through the year are.
struct MonthBars: View {
    let months: [FlownMonth]

    var body: some View {
        HStack(alignment: .bottom, spacing: VentiSpace.s1) {
            ForEach(Array(months.enumerated()), id: \.offset) { _, month in
                VStack(spacing: VentiSpace.s1) {
                    Color.clear
                        .frame(height: VentiSpace.s7)
                        .overlay(alignment: .bottom) {
                            Rectangle()
                                .fill(Palette.accent)
                                .frame(height: VentiSpace.s7 * min(max(month.share, 0), 1))
                        }
                        .overlay(alignment: .bottom) {
                            Divider()
                        }
                    Text(month.name)
                        .font(.ventiType(.caption))
                        .foregroundStyle(.venti(.textSecondary))
                        .lineLimit(1)
                        .minimumScaleFactor(0.5)
                }
                .frame(maxWidth: .infinity)
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(month.words)
                .accessibilityIdentifier("flown-month")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("flown-months")
    }
}

extension PublicFlight {
    /// One of a site's best flights as the screen of a public flight
    /// takes one: what it opens it by (the address, the file, the page
    /// and the pilot). The rest the screen reads from the flight's file.
    init(best line: SiteBestFlightLine) {
        self.init(
            id: line.id, handle: line.pilot.handle,
            pilot: line.pilot.name.isEmpty ? line.pilot.handle : line.pilot.name,
            startUnixS: 0, airtimeS: 0, takeoff: nil, trackLengthM: line.distanceM, maxAltitudeM: 0,
            maxClimbMs: 0, glider: nil, score: nil, outline: [], pagePath: line.pagePath, filePath: line.filePath
        )
    }
}

// MARK: Take-offs without a site

/// The places where pilots take off and no site stands, for the map's
/// layer "Take-offs without a site": off until the pilot switches it on,
/// and asked for only where the core gives a path for the part of the
/// map in view. Kept between launches where the model was given a store.
@MainActor
@Observable
final class CandidatesModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// How long an answer is used before the server is asked again.
    static let keepFor: TimeInterval = 60 * 60

    /// Whether the layer is on.
    private(set) var isOn: Bool
    /// The last answer for the part of the map in view; `nil` while the
    /// layer is off or nothing was answered.
    private(set) var answer: SiteCandidates?
    /// What stands over the map in place of the places: the core's words
    /// to zoom in, or what went wrong.
    private(set) var words: String?

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let store: UserDefaults?
    @ObservationIgnored private var answered: (area: Bounds, server: URL, at: Date)?
    @ObservationIgnored private var region: Bounds?
    @ObservationIgnored private var waiting: Task<Void, Never>?

    init(transport: @escaping Transport = WindModel.fetch, store: UserDefaults? = nil) {
        self.transport = transport
        self.store = store
        isOn = store?.bool(forKey: Preferences.mapShowsCandidates) ?? false
    }

    /// The places drawn: none while the layer is off.
    var candidates: [SiteCandidateView] {
        isOn ? answer?.candidates ?? [] : []
    }

    /// The pilot switches the layer on or off.
    func switchOn(_ isOn: Bool) {
        guard isOn != self.isOn else { return }
        self.isOn = isOn
        store?.set(isOn, forKey: Preferences.mapShowsCandidates)
        waiting?.cancel()
        if isOn {
            if let region {
                show(region)
            }
        } else {
            answer = nil
            words = nil
            answered = nil
        }
    }

    /// The map says which part of it is in view.
    func show(_ region: Bounds) {
        self.region = region
        guard isOn else { return }
        guard let server = server() else {
            answer = nil
            words = nil
            answered = nil
            return
        }
        // Whether this part may be asked about is the core's: farther out
        // its words stand over the map, and no place is drawn.
        do {
            _ = try Self.path(for: region)
        } catch {
            waiting?.cancel()
            answer = nil
            answered = nil
            words = error.words
            return
        }
        if let answered, answered.server == server, answered.area.contains(region),
           Date().timeIntervalSince(answered.at) < Self.keepFor
        {
            words = nil
            return
        }
        waiting?.cancel()
        waiting = Task { [weak self] in
            // The map reports every pause of a drag; wait until it rests.
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            await self?.load(region, from: server)
        }
    }

    /// Asks for an area: a little more than is in view where the core
    /// still gives a path for that, so that a short drag needs no request.
    func load(_ region: Bounds, from server: URL) async {
        let grown = region.grown(by: 0.25)
        let area = (try? Self.path(for: grown)) != nil ? grown : region
        do {
            let answer = try await Self.fetch(area, from: server, transport: transport)
            guard isOn, server == self.server() else { return }
            self.answer = answer
            answered = (area, server, Date())
            words = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            words = error is CoreError
                ? error.words
                : "Venti could not be reached for the take-offs without a site. \(error.localizedDescription)"
        }
    }

    /// The core's path for an area, or its words why it is not asked.
    static func path(for area: Bounds) throws -> String {
        try siteCandidatesPath(south: area.south, west: area.west, north: area.north, east: area.east)
    }

    /// Asks Venti for the places of an area and has the core read them.
    static func fetch(_ area: Bounds, from server: URL, transport: Transport) async throws -> SiteCandidates {
        let path = try path(for: area)
        guard let url = URL(string: server.absoluteString + path) else { throw URLError(.badURL) }
        let (status, body) = try await transport(url)
        return try siteCandidates(status: UInt16(clamping: status), body: body)
    }

    /// The answer narrowed to one of its places, for the core's offer to
    /// the pilot about that place alone.
    static func only(_ candidate: SiteCandidateView, of answer: SiteCandidates) -> SiteCandidates {
        var one = answer
        one.candidates = [candidate]
        one.more = false
        one.moreWords = ""
        return one
    }
}

/// A place of the layer, chosen on the map, for a sheet.
struct ChosenCandidate: Identifiable {
    let candidate: SiteCandidateView
    let answer: SiteCandidates
    var id: String { "\(candidate.position.lat),\(candidate.position.lon)" }
}

/// What a tap on a place of the layer opens: the core's title, its line,
/// its height, its season and months, and, where the core offers it to
/// this pilot, "Add this place" and "Not this one". Once added, the
/// sheet shows the new site's page.
struct CandidateSheet: View {
    let chosen: ChosenCandidate
    let done: () -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var offer: CandidateOffer?
    @State private var added: SiteWritten?

    var body: some View {
        let candidate = chosen.candidate
        NavigationStack {
            Group {
                if let added {
                    SiteScreen(site: added.site, said: added.words)
                } else {
                    VentiList {
                        Section {
                            Text(candidate.words)
                                .accessibilityIdentifier("candidate-words")
                            LabeledContent("Height", value: candidate.altitudeWords)
                                .accessibilityIdentifier("candidate-height")
                            if !candidate.seasonWords.isEmpty {
                                Text(candidate.seasonWords)
                                    .accessibilityIdentifier("candidate-season")
                            }
                            if !candidate.months.isEmpty {
                                MonthBars(months: candidate.months)
                                    .padding(.vertical, VentiSpace.s1)
                            }
                        } footer: {
                            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                                Text(FlightsAtSitesWords.core.layerAbout)
                                if !chosen.answer.terms.text.isEmpty {
                                    Text(chosen.answer.terms.text)
                                }
                            }
                        }
                        if let offer {
                            Section {
                                CandidateOfferView(
                                    offer: offer,
                                    onAdded: { written in added = written },
                                    onSetAside: done
                                )
                            }
                        }
                    }
                    .navigationTitle(FlightsAtSitesWords.core.candidateTitle)
                    .navigationBarTitleDisplayMode(.inline)
                }
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: done)
                }
            }
        }
        .task(id: chosen.id) {
            offer = await account.candidateOffers(
                CandidatesModel.only(candidate, of: chosen.answer), takesSitesFromPilots: wind.takesSitesFromPilots
            ).first
        }
    }
}

/// The core's offer to add a place the pilot took off at: its sentence,
/// the pilot's flight from there, "Add this place", which opens the form
/// for adding a site on the core's draft, and "Not this one", which sets
/// the place aside on this iPhone.
struct CandidateOfferView: View {
    let offer: CandidateOffer
    let onAdded: (SiteWritten) -> Void
    let onSetAside: () -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var isAdding = false

    var body: some View {
        let adding = account.adding(takesSitesFromPilots: wind.takesSitesFromPilots)
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                Text(offer.words)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("candidate-offer-words")
                Text(offer.flight.words)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
            }
            HStack(spacing: VentiSpace.s3) {
                Button {
                    account.setAside(offer.candidate)
                    onSetAside()
                } label: {
                    Text(offer.dismissWords)
                        .sentenceInAButton()
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.quiet))
                .accessibilityIdentifier("candidate-set-aside")
                Button {
                    isAdding = true
                } label: {
                    Text(offer.addWords)
                        .sentenceInAButton()
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .disabled(adding == nil)
                .accessibilityIdentifier("candidate-add")
            }
        }
        .padding(.vertical, VentiSpace.s1)
        .sheet(isPresented: $isAdding) {
            if let adding {
                SiteEditForm(adding: adding, draft: offer.draft) { written in
                    onAdded(written)
                }
            }
        }
    }
}
