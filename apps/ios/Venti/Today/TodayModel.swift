import CoreLocation
import Foundation
import Observation
import VentiCore

/// Today: the take-offs around a place, and what the forecast says about
/// flying from each.
///
/// What to ask the server, how to read its answers and what a forecast
/// says about a take-off are the core's. This class asks for the sites
/// around the place and for the forecast of each of the nearest
/// take-offs, and keeps what came back.
@MainActor
@Observable
final class TodayModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// A take-off and what the forecast says of it.
    struct Place: Identifiable, Equatable {
        let site: SiteView
        /// How far it is from the place asked about, metres.
        let distanceM: Double
        /// Absent until the server has answered, and when it could not.
        var outlook: SiteOutlook?

        var id: String { site.id }
    }

    enum State: Equatable {
        /// Nothing asked yet.
        case idle
        case loading
        case loaded
        /// No Venti to ask: its address is not set.
        case noServer
        case failed(String)
    }

    /// How many take-offs are shown: the nearest ones.
    static let takeoffsShown = 6
    /// How far from the place a take-off may lie, metres.
    static let reachM = 30_000.0
    /// How long an answer is shown before the server is asked again.
    static let keepFor: TimeInterval = 30 * 60

    private(set) var state = State.idle
    /// The take-offs: those with a name first, and the nearest first.
    private(set) var places: [Place] = []
    /// Whose data the forecasts are. The licences ask for this next to
    /// the data.
    private(set) var credits: [Credit] = []
    /// Where the take-offs were looked for, and when.
    private(set) var asked: (place: Coordinate, at: Date)?

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// The time by the server's clock, seconds since 1970-01-01 UTC: the
    /// hours that are gone are gone by that clock. The app sets this.
    @ObservationIgnored var clock: () async -> Int64 = { Int64(Date().timeIntervalSince1970) }
    /// How long to wait before asking again for a forecast the server is
    /// still fetching, and how often.
    @ObservationIgnored var forecastPause = Duration.seconds(5)
    static let forecastTries = 7

    @ObservationIgnored private let transport: Transport
    /// What was asked about single sites, for their own pages.
    @ObservationIgnored private var askedSites: [String: (outlook: SiteOutlook, at: Date)] = [:]

    init(transport: @escaping Transport = WindModel.fetch) {
        self.transport = transport
    }

    /// What the forecast says about one site, for its own page: what the
    /// list already has, what was asked not long ago, or a new answer.
    /// `nil` without a server and when no answer could be read.
    func outlook(of site: SiteView) async -> SiteOutlook? {
        if let listed = places.first(where: { $0.id == site.id })?.outlook {
            return listed
        }
        if let kept = askedSites[site.id], Date().timeIntervalSince(kept.at) < Self.keepFor {
            return kept.outlook
        }
        guard let server = server() else { return nil }
        let nowUnixS = await clock()
        guard let outlook = await outlook(for: site, from: server, nowUnixS: nowUnixS) else { return nil }
        askedSites[site.id] = (outlook, Date())
        return outlook
    }

    /// Whether what is shown was asked for this place not long ago.
    func isFresh(for place: Coordinate) -> Bool {
        guard let asked, state == .loaded else { return false }
        let moved = CLLocation(latitude: asked.place.lat, longitude: asked.place.lon)
            .distance(from: CLLocation(latitude: place.lat, longitude: place.lon))
        return moved < 5_000 && Date().timeIntervalSince(asked.at) < Self.keepFor
    }

    /// Asks for the take-offs around a place and the forecast of each.
    func load(around place: Coordinate) async {
        guard let server = server() else {
            places = []
            credits = []
            asked = nil
            state = .noServer
            return
        }
        if places.isEmpty {
            state = .loading
        }
        do {
            let found = try await takeoffs(around: place, from: server)
            let nowUnixS = await clock()
            // The list at once, each forecast as it arrives.
            places = found
            asked = (place, Date())
            state = .loaded
            await withTaskGroup(of: (String, SiteOutlook?).self) { group in
                for takeoff in found {
                    group.addTask { [weak self] in
                        (takeoff.id, await self?.outlook(for: takeoff.site, from: server, nowUnixS: nowUnixS))
                    }
                }
                for await (id, outlook) in group {
                    guard let outlook, let index = places.firstIndex(where: { $0.id == id }) else { continue }
                    places[index].outlook = outlook
                    if credits.isEmpty {
                        credits = outlook.credits
                    }
                }
            }
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            state = .failed(
                error is CoreError
                    ? error.words
                    : "Venti could not be reached for today's forecast. \(error.localizedDescription)"
            )
        }
    }

    /// The take-offs within reach of a place.
    private func takeoffs(around place: Coordinate, from server: URL) async throws -> [Place] {
        // A box that holds the circle of the reach.
        let north = Self.reachM / 111_195
        let east = north / max(cos(place.lat * .pi / 180), 0.2)
        let area = Bounds(
            south: place.lat - north, west: place.lon - east, north: place.lat + north, east: place.lon + east
        )
        let path = try sitesPath(south: area.south, west: area.west, north: area.north, east: area.east)
        guard let url = URL(string: server.absoluteString + path) else { return [] }
        let (status, body) = try await transport(url)
        let map = try sitesMap(
            south: area.south, west: area.west, north: area.north, east: area.east,
            status: UInt16(clamping: status), body: body
        )
        let here = CLLocation(latitude: place.lat, longitude: place.lon)
        var near: [Place] = []
        for site in map.sites where site.uses.contains(.takeoff) && !site.discouraged {
            let distance = here.distance(
                from: CLLocation(latitude: site.position.lat, longitude: site.position.lon)
            )
            if distance <= Self.reachM {
                near.append(Place(site: site, distanceM: distance))
            }
        }
        // Those with a name of their own first: "Take-off" tells a pilot
        // nothing. Among them, and among the rest, the nearest.
        near.sort { (one: Place, other: Place) -> Bool in
            if one.site.isNamed != other.site.isNamed {
                return one.site.isNamed
            }
            return one.distanceM < other.distanceM
        }
        return Array(near.prefix(Self.takeoffsShown))
    }

    /// What the forecast says about a take-off. The first forecast of a
    /// place is one the server has to fetch: it answers "not yet", and is
    /// asked again. `nil` when no answer could be read.
    private func outlook(for site: SiteView, from server: URL, nowUnixS: Int64) async -> SiteOutlook? {
        guard let path = try? forecastPath(lat: site.position.lat, lon: site.position.lon),
              let url = URL(string: server.absoluteString + path)
        else { return nil }
        var tries = 1
        while true {
            guard let (status, body) = try? await transport(url) else { return nil }
            if status == 503, tries < Self.forecastTries {
                tries += 1
                try? await Task.sleep(for: forecastPause)
                continue
            }
            let offset = TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: TimeInterval(nowUnixS)))
            return try? siteOutlook(
                status: UInt16(clamping: status), body: body,
                utcOffsetS: Int32(offset),
                wind: site.wind, elevationM: site.elevationM,
                nowUnixS: nowUnixS
            )
        }
    }
}

/// Where the pilot is, for the take-offs around: asked of the system only
/// when the pilot says so, and read without asking once it is allowed.
@MainActor
@Observable
final class PilotPosition: NSObject, @preconcurrency CLLocationManagerDelegate {
    /// The latest position, once there is one.
    private(set) var place: Coordinate?
    /// Whether the pilot has said no, here or in the system's settings.
    private(set) var isDenied = false

    @ObservationIgnored private let manager = CLLocationManager()

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyKilometer
    }

    /// Whether the app may read the position without asking.
    var isAllowed: Bool {
        manager.authorizationStatus == .authorizedWhenInUse || manager.authorizationStatus == .authorizedAlways
    }

    /// Reads the position if the app may already; asks nobody.
    func readIfAllowed() {
        guard isAllowed else { return }
        if let known = manager.location {
            place = Coordinate(lat: known.coordinate.latitude, lon: known.coordinate.longitude)
        }
        manager.requestLocation()
    }

    /// The pilot asked for the take-offs around them: the system asks
    /// whether the app may know where that is.
    func ask() {
        if isAllowed {
            readIfAllowed()
        } else {
            manager.requestWhenInUseAuthorization()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        isDenied = manager.authorizationStatus == .denied || manager.authorizationStatus == .restricted
        readIfAllowed()
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        if let latest = locations.last, latest.horizontalAccuracy >= 0 {
            place = Coordinate(lat: latest.coordinate.latitude, lon: latest.coordinate.longitude)
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: any Error) {
        // No position this time: the screen keeps the place it had.
    }
}
