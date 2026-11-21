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
    /// Whether this Venti takes sites from pilots, as its status says:
    /// false until it has said so, and for one that takes none.
    private(set) var takesSitesFromPilots = false

    /// Venti's address: the one the app was built with (`VentiServer`).
    let address: String

    @ObservationIgnored private let transport: Transport
    /// The part of the map in view.
    @ObservationIgnored private var wanted: Bounds?
    /// The area the stations were asked for, and when the answer came.
    @ObservationIgnored private var answered: (area: Bounds, at: Date)?
    /// The server's clock minus this device's, and when it was read.
    @ObservationIgnored private var clock: (ahead: TimeInterval, readAt: Date)?
    @ObservationIgnored private var waiting: Task<Void, Never>?

    /// - Parameter address: the server to use: Venti's, unless a test
    ///   names another.
    init(address: String = VentiServer.address, transport: @escaping Transport = WindModel.fetch) {
        self.address = address
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
                takesSitesFromPilots = state.takesSitesFromPilots
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

    /// The current time by the server's clock, read from the server first
    /// if it has not been yet. This iPhone's own time when there is no
    /// server or no answer.
    func serverNow() async -> Int64 {
        if clock == nil, let server,
           let (status, body) = try? await get(serverStatusPath(), from: server),
           let state = try? serverStatus(status: status, body: body)
        {
            clock = (TimeInterval(state.timeUnixS) - Date().timeIntervalSince1970, Date())
            takesSitesFromPilots = state.takesSitesFromPilots
        }
        return now
    }

    /// The stations around a glider in the air and around the place its
    /// pilot means to land, with whose data they are: asked for once,
    /// beside what the map tab shows. `nil` without a server or without
    /// an answer, which in the air is nothing to tell the pilot about.
    func stations(around place: Coordinate, andAround landing: Coordinate?) async -> WindMap? {
        guard let server else { return nil }
        // Some eighty kilometres every way from the glider: the stations
        // of the weather services stand far apart.
        var area = Bounds(
            south: place.lat - 0.75, west: place.lon - 1.1, north: place.lat + 0.75, east: place.lon + 1.1
        )
        if let landing {
            area = Bounds(
                south: min(area.south, landing.lat - 0.05), west: min(area.west, landing.lon - 0.07),
                north: max(area.north, landing.lat + 0.05), east: max(area.east, landing.lon + 0.07)
            )
        }
        do {
            // Ages are measured against the server's clock. A pilot who
            // went straight to the cockpit has not read it yet.
            if clock == nil {
                let (status, body) = try await get(serverStatusPath(), from: server)
                let state = try serverStatus(status: status, body: body)
                clock = (TimeInterval(state.timeUnixS) - Date().timeIntervalSince1970, Date())
            }
            let path = try windMapPath(south: area.south, west: area.west, north: area.north, east: area.east)
            let (status, body) = try await get(path, from: server)
            return try windMap(status: status, body: body, nowUnixS: now)
        } catch {
            return nil
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

    /// The offset of this iPhone's clock from UTC, by which a station's
    /// days are cut.
    static var utcOffsetS: Int32 {
        Int32(TimeZone.current.secondsFromGMT())
    }

    /// One past day of a station's readings, by its date ("2026-11-18").
    func day(of station: WindStation, date: String) async throws -> StationDay {
        guard let server else {
            throw URLError(.badURL)
        }
        let path = try stationDayPath(stationId: station.id, date: date, utcOffsetS: Self.utcOffsetS)
        let (status, body) = try await get(path, from: server)
        return try stationDay(status: status, body: body, date: date, utcOffsetS: Self.utcOffsetS)
    }

    /// How long to wait before asking again for a forecast the server is
    /// still fetching, and how often to ask at most.
    @ObservationIgnored var forecastPause = Duration.seconds(5)
    static let forecastTries = 7

    /// What the weather models say about a place for today and the next
    /// days. The days are those of this iPhone's clock.
    ///
    /// The first forecast of a place is one the server has to fetch, and
    /// until it has it the server answers "not yet": it is asked again
    /// every few seconds, for about half a minute.
    func forecast(at place: Coordinate) async throws -> ForecastView {
        guard let server else {
            throw URLError(.badURL)
        }
        let path = try forecastPath(lat: place.lat, lon: place.lon)
        var (status, body) = try await get(path, from: server)
        var tries = 1
        while status == 503, tries < Self.forecastTries {
            try await Task.sleep(for: forecastPause)
            (status, body) = try await get(path, from: server)
            tries += 1
        }
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
