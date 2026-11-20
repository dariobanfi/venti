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

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }

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
                : "The server could not be reached for other pilots' flights. \(error.localizedDescription)"
        }
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
