import Foundation
import Observation
import VentiCore

/// Other pilots' flights: the flights the pilots of the Venti server made
/// public, in one list.
///
/// What to ask the server and how to read its answer is the core's. This
/// class sends the requests, keeps the pages it was given, and fetches a
/// flight's file when the pilot opens one. Nothing of another pilot's is
/// kept on the iPhone.
@MainActor
@Observable
final class CommunityModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// The flights listed so far, in the order shown.
    private(set) var flights: [PublicFlight] = []
    /// The order they are in.
    private(set) var order = PublicFlightsOrder.newest
    /// Whether a page is being fetched.
    private(set) var isLoading = false
    /// Whether the server has more than is listed.
    private(set) var hasMore = false
    /// Why the list is missing, in words for the pilot.
    private(set) var problem: String?

    /// What the pilot sees of the ranking of the server's pilots.
    struct Ranked: Equatable {
        /// The seasons to choose from, the newest first. None before the
        /// ranking was first asked for.
        var seasons: [RankingSeason] = []
        /// The name of the season the ranking is of.
        var season = ""
        /// The month of that season the ranking is of, as the core
        /// writes it ("2026-05"). `nil` for the whole season.
        var month: String?
        /// The years that have a ranking, each with its months that have
        /// one: the server's, the same whichever the ranking is of.
        var periods: [RankingYearChoice] = []
        /// The pilots, the one with the most points first.
        var pilots: [RankedPilot] = []
        /// The rule in a sentence, for under the list.
        var rule = ""
        /// Whether the ranking is being fetched.
        var isLoading = false
        /// Why it is missing, in words for the pilot.
        var problem: String?

        /// The months of the season shown that have a ranking, the
        /// newest first, in the core's names. None for all years.
        var months: [RankingMonthChoice] {
            periods.first { $0.name == season }?.months ?? []
        }

        /// What the answer's ranking is of, in the core's words: "May
        /// 2026", "2026", "All years". Empty before the first answer.
        var periodTitle = ""

        /// The ranking's heading: the core's title of the answer's
        /// period, and until it has come the name of what was asked for.
        var title: String {
            periodTitle.isEmpty ? (months.first { $0.month == month }?.title ?? season) : periodTitle
        }

        /// The whole of the season shown as the first choice beside its
        /// months, in the core's words: "All of 2026".
        var wholeName: String {
            periods.first { $0.name == season }?.wholeName ?? season
        }
    }

    /// The ranking of the season last asked for.
    private(set) var ranked = Ranked()

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// Venti's clock for the years the ranking offers before the server
    /// names its own, or 0 where it is not known (the core then offers
    /// all years). The app sets this to the server's time as read.
    @ObservationIgnored var clock: () async -> Int64 = { 0 }

    @ObservationIgnored private let transport: Transport
    /// What to ask the next page with.
    @ObservationIgnored private var next: String?
    /// Which server and order the list is from.
    @ObservationIgnored private var listed: (server: URL, order: PublicFlightsOrder)?

    init(transport: @escaping Transport = CommunityModel.fetch) {
        self.transport = transport
    }

    /// Loads the list, unless it is there already for the server that is
    /// set and the order chosen.
    func loadIfNeeded() async {
        guard let server = server() else {
            forget()
            return
        }
        if let listed, listed.server == server, listed.order == order {
            return
        }
        await load(fromTheTop: true)
    }

    /// Shows the list in another order, from its top.
    func show(_ order: PublicFlightsOrder) async {
        guard order != self.order else { return }
        self.order = order
        flights = []
        hasMore = false
        await load(fromTheTop: true)
    }

    /// Asks again from the top: the pilot pulled the list down.
    func refresh() async {
        await load(fromTheTop: true)
    }

    /// The next page, asked for when the last row comes into view.
    func loadMore() async {
        guard hasMore else { return }
        await load(fromTheTop: false)
    }

    private func load(fromTheTop: Bool) async {
        guard let server = server() else {
            forget()
            return
        }
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        let order = self.order
        do {
            let path = try publicFlightsPath(
                order: order, area: nil, fromUnixS: nil, toUnixS: nil, after: fromTheTop ? nil : next
            )
            guard let url = URL(string: server.absoluteString + path) else { return }
            let (status, body) = try await transport(url)
            let page = try publicFlights(status: UInt16(clamping: status), body: body)
            // The pilot may have changed the server or the order while
            // this was under way.
            guard server == self.server(), order == self.order else { return }
            flights = fromTheTop ? page.flights : flights + page.flights
            next = page.next
            hasMore = page.next != nil
            listed = (server, order)
            problem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = error is CoreError
                ? error.words
                : pilotsWords().unreachable
        }
    }

    /// Fetches the ranking of a season, by the name the core gives it:
    /// the whole season, whichever month of another was shown. Without a
    /// name it is what was last shown, season and month, or the newest.
    func rank(season: String? = nil, now: Date? = nil) async {
        await rank(season: season, month: season == nil ? ranked.month : nil, now: now)
    }

    /// Fetches the ranking of a month of the season shown, as the core
    /// writes the month ("2026-05"); `nil` is the whole season again.
    func rank(month: String?, now: Date? = nil) async {
        await rank(season: nil, month: month, now: now)
    }

    private func rank(season: String?, month: String?, now: Date?) async {
        guard let server = server() else {
            ranked = Ranked()
            return
        }
        let nowUnixS = if let now { Int64(now.timeIntervalSince1970) } else { await clock() }
        let byTheClock = rankingSeasons(nowUnixS: nowUnixS)
        let wanted = season ?? ranked.season
        let offered = ranked.seasons.isEmpty ? byTheClock : ranked.seasons
        guard let chosen = offered.first(where: { $0.name == wanted }) ?? byTheClock.first else { return }
        if chosen.name != ranked.season || month != ranked.month {
            // Another season's or month's pilots are not this one's.
            ranked.pilots = []
            ranked.rule = ""
            ranked.periodTitle = ""
        }
        ranked.seasons = offered
        ranked.season = chosen.name
        ranked.month = month
        ranked.problem = nil
        ranked.isLoading = true
        do {
            let path = if let month {
                try rankingMonthPath(area: nil, month: month)
            } else {
                try rankingPath(area: nil, fromUnixS: chosen.fromUnixS, toUnixS: chosen.toUnixS)
            }
            guard let url = URL(string: server.absoluteString + path) else {
                ranked.isLoading = false
                return
            }
            let (status, body) = try await transport(url)
            let answer = try VentiCore.ranking(status: UInt16(clamping: status), body: body)
            // The pilot may have chosen another season, month or server
            // while this was under way: that question's answer sets the
            // rest.
            guard server == self.server(), ranked.season == chosen.name, ranked.month == month else { return }
            ranked.pilots = answer.pilots
            ranked.rule = answer.rule
            ranked.periodTitle = answer.period.title
            ranked.periods = answer.periods
            ranked.seasons = Self.seasons(having: answer.periods, shown: chosen, byTheClock: byTheClock)
            ranked.isLoading = false
        } catch {
            guard ranked.season == chosen.name, ranked.month == month else { return }
            ranked.isLoading = false
            if error is CancellationError || (error as? URLError)?.code == .cancelled {
                return
            }
            ranked.problem = error is CoreError
                ? error.words
                : pilotsWords().unreachable
        }
    }

    /// The seasons a ranking is offered for once the server has said
    /// which years have one: those years, the newest first, the season
    /// shown where it is not one of them, and all years. From a server
    /// that does not say, the core's years by the clock.
    static func seasons(
        having periods: [RankingYearChoice], shown: RankingSeason, byTheClock: [RankingSeason]
    ) -> [RankingSeason] {
        guard !periods.isEmpty else { return byTheClock }
        var years = periods.map {
            RankingSeason(name: $0.name, fromUnixS: $0.fromUnixS, toUnixS: $0.toUnixS)
        }
        if shown.fromUnixS != nil, !years.contains(where: { $0.name == shown.name }) {
            years.append(shown)
        }
        years.sort { ($0.fromUnixS ?? 0) > ($1.fromUnixS ?? 0) }
        return years + byTheClock.filter { $0.fromUnixS == nil }
    }

    /// Fetches a flight's file and prepares it for display, like one of
    /// the pilot's own. Nothing is kept.
    func open(_ flight: PublicFlight) async throws -> (FlightView, FlightAnalysis?) {
        guard let server = server(), let url = URL(string: server.absoluteString + flight.filePath) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        guard (200..<300).contains(status) else {
            throw URLError(.fileDoesNotExist)
        }
        // Flight files are plain ASCII by specification, but recorders
        // write pilot names in whatever encoding they like.
        let text = String(data: body, encoding: .utf8) ?? String(decoding: body, as: Unicode.ASCII.self)
        return try await Task.detached(priority: .userInitiated) { () throws -> (FlightView, FlightAnalysis?) in
            let view = try flightFromFile(igcText: text)
            // Without the analysis the flight is still shown in full.
            return (view, try? flightAnalysisFromFile(igcText: text))
        }.value
    }

    /// The public flights that took off inside a part of the map and
    /// began in a span of time, the newest first: what was flown around a
    /// flight on its day. The first page of them.
    func flights(around area: Bounds, fromUnixS: Int64, toUnixS: Int64) async throws -> [PublicFlight] {
        let path = try publicFlightsPath(
            order: .newest, area: area, fromUnixS: fromUnixS, toUnixS: toUnixS, after: nil
        )
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try publicFlights(status: UInt16(clamping: status), body: body).flights
    }

    /// One page of a pilot's public flights, the newest first. `after` is
    /// the `next` of the page before.
    func pilot(_ handle: String, after: String? = nil) async throws -> PilotPage {
        let path = try pilotPagePath(handle: handle, after: after)
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try pilotPage(status: UInt16(clamping: status), body: body)
    }

    /// One page of a pilot's public flights that began in a season, the
    /// newest first: the season's bounds are those of the pilot's totals.
    /// `after` is the `next` of the page before.
    func pilot(
        _ handle: String, fromUnixS: Int64?, toUnixS: Int64?, after: String? = nil
    ) async throws -> PublicFlights {
        let path = try pilotFlightsPath(handle: handle, fromUnixS: fromUnixS, toUnixS: toUnixS, after: after)
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try publicFlights(status: UInt16(clamping: status), body: body)
    }

    /// The pilots whose handle or name holds a text, as the core lists
    /// them. Fewer than two letters are not asked for: the core refuses
    /// them in words, which are thrown.
    func pilots(named text: String) async throws -> PilotsFound {
        let path = try pilotsSearchPath(text: text)
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try pilotsFound(status: UInt16(clamping: status), body: body)
    }

    /// The pilots who shared flights from near a place, as the core lists
    /// them: within Venti's own distance, the last months.
    func pilotsNear(_ place: Coordinate) async throws -> PilotsNear {
        let path = try pilotsNearPath(lat: place.lat, lon: place.lon, radiusKm: nil)
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try VentiCore.pilotsNear(status: UInt16(clamping: status), body: body)
    }

    /// What a pilot's public flights add up to, overall and season by
    /// season.
    func statistics(of handle: String) async throws -> PilotStatistics {
        let path = try pilotStatisticsPath(handle: handle)
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try pilotStatistics(status: UInt16(clamping: status), body: body)
    }

    /// One public flight by its address on the server, as a row of the
    /// list has it: for what names a flight by its address alone.
    func flight(_ id: String) async throws -> PublicFlight {
        let path = try publicFlightPath(id: id)
        guard let server = server(), let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return try publicFlight(status: UInt16(clamping: status), body: body)
    }

    /// Why a flight could not be opened, in words.
    static func words(for error: any Error) -> String {
        if error is CoreError {
            return error.words
        }
        if (error as? URLError)?.code == .fileDoesNotExist {
            return "This flight is no longer public: its pilot took it back, or deleted it."
        }
        return "The flight could not be fetched. \(error.localizedDescription)"
    }

    private func forget() {
        if !flights.isEmpty || listed != nil {
            flights = []
            next = nil
            hasMore = false
            listed = nil
        }
        problem = nil
    }

    /// Fetches with the system's networking. A flight's file is no JSON,
    /// so nothing is said about what is accepted.
    nonisolated static func fetch(_ url: URL) async throws -> (Int, Data) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 30
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }
}
