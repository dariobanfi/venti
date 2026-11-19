import Foundation
import Observation
import VentiCore

/// Live wind for the map, from the Venti server the pilot has set.
///
/// What to ask the server and how to read its answers is the core's. This
/// class sends the requests, keeps the newest answer for the part of the map
/// in view, and asks again every minute while the map is on screen.
@MainActor
@Observable
final class WindModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// How often the wind is asked for again, in seconds. The server lets
    /// an answer be reused for this long, and stations report every ten
    /// minutes.
    static let refreshInterval: TimeInterval = 60

    /// After this long without an answer the stations are taken off the
    /// map: an arrow must not outlive the wind it stood for.
    static let keepWithoutAnswer: TimeInterval = 15 * 60

    /// The stations around the part of the map in view.
    private(set) var stations: [WindStation] = []
    /// Whose data they are. The licences ask for this next to the data.
    private(set) var credits: [Credit] = []
    /// Whether the area holds more stations than the server lists at once.
    private(set) var isTruncated = false
    /// Why the wind is missing or not current, in words for the pilot.
    private(set) var problem: String?
    /// The providers the server has not heard from lately.
    private(set) var lateProviders: [String] = []

    /// The server's address as the pilot wrote it. Empty for none.
    private(set) var address: String

    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let remembersAddress: Bool
    /// The part of the map in view.
    @ObservationIgnored private var wanted: Bounds?
    /// The area the stations were asked for, and when the answer came.
    @ObservationIgnored private var answered: (area: Bounds, at: Date)?
    /// The server's clock minus this device's, and when it was read.
    @ObservationIgnored private var clock: (ahead: TimeInterval, readAt: Date)?
    @ObservationIgnored private var waiting: Task<Void, Never>?

    /// - Parameter address: the server to use. Without one, the address the
    ///   pilot saved in the settings is used and changes to it are saved.
    init(address: String? = nil, transport: @escaping Transport = WindModel.fetch) {
        self.address = address ?? UserDefaults.standard.string(forKey: Preferences.server) ?? ""
        remembersAddress = address == nil
        self.transport = transport
    }

    /// The server to ask, if the address is one.
    var server: URL? { Self.server(from: address) }

    /// The current time by the server's clock, which the ages of readings
    /// are measured against. A phone's clock can be wrong.
    var now: Int64 {
        Int64((Date().timeIntervalSince1970 + (clock?.ahead ?? 0)).rounded())
    }

    /// Reads an address as people write them: with or without `https://`,
    /// with or without a slash at the end.
    nonisolated static func server(from address: String) -> URL? {
        var text = address.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return nil }
        if !text.contains("://") {
            text = "https://" + text
        }
        while text.hasSuffix("/") {
            text.removeLast()
        }
        guard let url = URL(string: text),
              let scheme = url.scheme?.lowercased(), scheme == "https" || scheme == "http",
              let host = url.host(), !host.isEmpty
        else { return nil }
        return url
    }

    /// The map says which part of it is in view. The server is asked unless
    /// that part is covered by a recent answer.
    func show(_ region: Bounds) {
        wanted = region
        guard server != nil else { return }
        if let answered, !isTruncated, answered.area.contains(region),
           Date().timeIntervalSince(answered.at) < Self.refreshInterval
        {
            return
        }
        waiting?.cancel()
        waiting = Task { [weak self] in
            // The map reports every pause of a drag; wait until it rests.
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            await self?.load()
        }
    }

    /// Keeps the wind current for as long as the calling task lives: the
    /// map screen runs this while it is on screen.
    func keepCurrent() async {
        while !Task.isCancelled {
            await load()
            try? await Task.sleep(for: .seconds(Self.refreshInterval))
        }
    }

    /// Asks the server for the stations around the part of the map in view.
    func load() async {
        guard let server, let region = wanted else { return }
        // Somewhat more than is in view, so a small drag needs no request.
        let area = region.grown(by: 0.25)
        do {
            if clock.map({ Date().timeIntervalSince($0.readAt) >= Self.refreshInterval }) ?? true {
                let (status, body) = try await get(serverStatusPath(), from: server)
                let state = try serverStatus(status: status, body: body)
                clock = (TimeInterval(state.timeUnixS) - Date().timeIntervalSince1970, Date())
                lateProviders = state.lateProviders
            }
            let path = try windMapPath(south: area.south, west: area.west, north: area.north, east: area.east)
            let (status, body) = try await get(path, from: server)
            let map = try windMap(status: status, body: body, nowUnixS: now)
            // The pilot may have changed the address while this was under way.
            guard server == self.server else { return }
            stations = map.stations
            credits = map.credits
            isTruncated = map.truncated
            answered = (area, Date())
            problem = nil
        } catch is CancellationError {
            return
        } catch {
            guard server == self.server else { return }
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = error is CoreError
                ? error.words
                : "The wind server could not be reached. \(error.localizedDescription)"
            if answered.map({ Date().timeIntervalSince($0.at) > Self.keepWithoutAnswer }) ?? true {
                stations = []
                credits = []
            }
        }
    }

    /// What a station measured over the last 24 hours.
    func history(of station: WindStation) async throws -> StationHistory {
        guard let server else {
            throw URLError(.badURL)
        }
        let (status, body) = try await get(stationHistoryPath(stationId: station.id), from: server)
        return try stationHistory(status: status, body: body, nowUnixS: now)
    }

    /// What the weather models say about a place for today and the next
    /// days. The days are those of this iPhone's clock.
    func forecast(at place: Coordinate) async throws -> ForecastView {
        guard let server else {
            throw URLError(.badURL)
        }
        let (status, body) = try await get(forecastPath(lat: place.lat, lon: place.lon), from: server)
        return try VentiCore.forecast(
            status: status, body: body, utcOffsetS: Int32(TimeZone.current.secondsFromGMT())
        )
    }

    private func get(_ path: String, from server: URL) async throws -> (UInt16, Data) {
        guard let url = URL(string: server.absoluteString + path) else {
            throw URLError(.badURL)
        }
        let (status, body) = try await transport(url)
        return (UInt16(clamping: status), body)
    }

    /// Changes the server, and drops everything learned from the old one.
    func use(address: String) {
        guard address != self.address else { return }
        self.address = address
        if remembersAddress {
            UserDefaults.standard.set(address, forKey: Preferences.server)
        }
        waiting?.cancel()
        stations = []
        credits = []
        isTruncated = false
        problem = nil
        lateProviders = []
        answered = nil
        clock = nil
    }

    /// Fetches with the system's own networking and its cache, which
    /// honours the server's instructions on how long an answer may be
    /// reused.
    nonisolated static func fetch(_ url: URL) async throws -> (Int, Data) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 20
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }
}

extension Bounds {
    /// Whether another box lies wholly inside this one.
    func contains(_ other: Bounds) -> Bool {
        south <= other.south && north >= other.north && west <= other.west && east >= other.east
    }

    /// The box widened on every side by a share of its own size.
    func grown(by share: Double) -> Bounds {
        let height = (north - south) * share
        let width = (east - west) * share
        return Bounds(south: south - height, west: west - width, north: north + height, east: east + width)
    }
}
