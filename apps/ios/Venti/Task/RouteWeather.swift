import Foundation
import Observation
import VentiCore

/// The weather along a route drawn on the map: the server's forecast at
/// its places for a day (`routeWeatherPath`, `RouteForecast.read`), and
/// the ground under it, for the core to say when the pilot is where, what
/// the forecast has there then and the height to leave each place at
/// (`routeAlong`).
///
/// What to ask, every rule and every word about the weather are the
/// core's. This class asks once for each route and day (the answer holds
/// all of the day's hours, so a later start reads the same answer),
/// brings the ground once for each route, and drops an answer that comes
/// late for a route or a day no longer drawn.
@MainActor
@Observable
final class RouteWeatherModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// The forecast along the route and day last asked for; `nil` while
    /// there is none.
    private(set) var forecast: RouteForecast?
    /// The ground under the route last shown; `nil` while there is none.
    private(set) var heights: Terrain?
    /// Whether the server is being asked.
    private(set) var isAsking = false
    /// Why the last asking brought no forecast, in words for the pilot;
    /// `nil` after one that did.
    private(set) var problem: String?

    /// The server to ask; `nil` for none.
    @ObservationIgnored var server: () -> URL? = { nil }
    @ObservationIgnored private let transport: Transport
    /// The ground under an area; `nil` for none, and the heights then say
    /// it is not loaded.
    @ObservationIgnored var ground: (@MainActor (Bounds) async -> Terrain)?
    /// What the forecast held is for, and what was asked last: a late
    /// answer for something else is dropped.
    @ObservationIgnored private var heldFor: String?
    @ObservationIgnored private var wanted: String?
    /// Which places the ground held is under, and the ground last asked
    /// for.
    @ObservationIgnored private var heightsFor: [Coordinate]?
    @ObservationIgnored private var groundWanted: [Coordinate]?

    init(transport: @escaping Transport = WindModel.fetch, ground: (@MainActor (Bounds) async -> Terrain)? = nil) {
        self.transport = transport
        self.ground = ground
    }

    /// Brings the forecast along these places for a day (`date` as the
    /// thermal map names it) and the ground under them: from what is
    /// held, else from the server and the terrain.
    func show(_ places: [Coordinate], on date: String) async {
        guard places.count >= 2 else {
            drop()
            return
        }
        // The forecast first: it is one asking, and the ground may be
        // dozens of tiles. The heights follow when it has come.
        await bringForecast(along: places, on: date)
        await bringGround(under: places)
    }

    private func bringForecast(along places: [Coordinate], on date: String) async {
        guard let server = server() else {
            drop()
            problem = RouteWeatherWords.noServer
            return
        }
        let path: String
        do {
            path = try routeWeatherPath(places: Self.places(places), date: date)
        } catch {
            drop()
            problem = error.words
            return
        }
        wanted = path
        if path == heldFor, forecast != nil {
            return
        }
        // Another route or day: the forecast of the one before is not
        // this one's.
        forecast = nil
        heldFor = nil
        isAsking = true
        problem = nil
        defer {
            if wanted == path {
                isAsking = false
            }
        }
        do {
            guard let url = URL(string: server.absoluteString + path) else {
                throw URLError(.badURL)
            }
            let (status, body) = try await transport(url)
            let read = try await Task.detached(priority: .userInitiated) {
                try RouteForecast.read(status: UInt16(clamping: status), body: body)
            }.value
            guard wanted == path else { return }
            forecast = read
            heldFor = path
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled || wanted != path {
                return
            }
            problem = error is CoreError
                ? error.words
                : "\(RouteWeatherWords.unreachable) \(error.localizedDescription)"
        }
    }

    private func bringGround(under places: [Coordinate]) async {
        guard let ground, places != heightsFor else { return }
        groundWanted = places
        let brought = await ground(RouteWeatherWords.around(places))
        // A showing that was cancelled (the day came while the ground was
        // on its way) leaves it to the showing after it, which brings it
        // again; a route no longer drawn has no use for it.
        guard groundWanted == places, !Task.isCancelled else { return }
        heights = brought
        heightsFor = places
    }

    /// The route with the weather along it, as the core says it: with the
    /// forecast and the ground held, or without them where there are
    /// none. Its times are on the iPhone's clock.
    func along(
        _ places: [Coordinate], startUnixS: Int64, speedKmh: Int, glideRatio: Double,
        zone: TimeZone = .current
    ) -> RouteAlong {
        let offset = zone.secondsFromGMT(for: Date(timeIntervalSince1970: TimeInterval(startUnixS)))
        return routeAlong(
            setting: RouteSetting(
                places: Self.places(places),
                startUnixS: startUnixS,
                groundSpeedKmh: Double(speedKmh),
                glideRatio: glideRatio,
                flyingHeightM: nil
            ),
            forecast: forecast,
            terrain: heights,
            utcOffsetS: Int32(offset)
        )
    }

    /// Forgets the forecast, so that "Try again" asks anew.
    func forgetForecast() {
        forecast = nil
        heldFor = nil
    }

    private func drop() {
        wanted = nil
        forecast = nil
        heldFor = nil
        isAsking = false
        problem = nil
    }

    private static func places(_ places: [Coordinate]) -> [RoutePlace] {
        places.map { RoutePlace(lat: $0.lat, lon: $0.lon, name: nil) }
    }
}

/// What the weather along a route says that is not the core's, and the
/// settings' steps.
enum RouteWeatherWords {
    static let title = "Weather along the route"
    static let asking = "Asking Venti for the weather along the route."
    static let unreachable = "Venti could not be reached for the weather along the route."
    static let noServer = "Without Venti there is no forecast along the route."

    /// The start moves by this, minutes, within the day.
    static let startStepMinutes = 30
    static let starts = 0...(24 * 60 - startStepMinutes)

    /// The speed over the ground moves by this, km/h, between these: the
    /// core's own limits.
    static let speedStepKmh = 5
    static let speedsKmh = 5...80
    static let speedKmh = 20

    /// The glide moves by this, between these: as in the settings.
    static let glideStep = 0.5
    static let glideRatios = 4.0...14.0

    /// The next full hour after a moment on the pilot's clock, as a
    /// minute of the day; the last hour of the day at the latest.
    static func nextFullHour(after unixS: Int64, zone: TimeZone = .current) -> Int {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let hour = calendar.component(.hour, from: Date(timeIntervalSince1970: TimeInterval(unixS)))
        return min(hour + 1, 23) * 60
    }

    /// The day of a moment on the pilot's clock, as the thermal map names
    /// a day: "2026-11-22".
    static func date(of unixS: Int64, zone: TimeZone = .current) -> String {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let day = calendar.dateComponents([.year, .month, .day], from: Date(timeIntervalSince1970: TimeInterval(unixS)))
        return String(format: "%04d-%02d-%02d", day.year ?? 0, day.month ?? 0, day.day ?? 0)
    }

    /// A minute of a day on the pilot's clock, Unix seconds; `nil` for a
    /// date that is none.
    static func startUnixS(date: String, minuteOfDay: Int, zone: TimeZone = .current) -> Int64? {
        let parts = date.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = zone
        let components = DateComponents(
            year: parts[0], month: parts[1], day: parts[2], hour: minuteOfDay / 60, minute: minuteOfDay % 60
        )
        return calendar.date(from: components).map { Int64($0.timeIntervalSince1970) }
    }

    /// The area round a route's places, a little wider than they reach:
    /// the ground the lines between them cross.
    static func around(_ places: [Coordinate]) -> Bounds {
        let margin = 0.02
        return Bounds(
            south: (places.map(\.lat).min() ?? 0) - margin,
            west: (places.map(\.lon).min() ?? 0) - margin,
            north: (places.map(\.lat).max() ?? 0) + margin,
            east: (places.map(\.lon).max() ?? 0) + margin
        )
    }
}
