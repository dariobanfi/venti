import CoreLocation
import Foundation
import Observation
import VentiCore

/// Today: the pilot's own take-offs and those around a place, and what the
/// forecast says about flying from each.
///
/// Which take-offs those are, what to ask the server, how to read its
/// answers and what a forecast says about a take-off are the core's. This
/// class asks for the sites of the area the core names, hands the answer
/// to the core, asks for the forecast of each take-off it gives back, and
/// keeps what came.
@MainActor
@Observable
final class TodayModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// A take-off and what the forecast says of it.
    struct Place: Identifiable, Equatable {
        let site: SiteView
        /// Whether it is one of the pilot's own.
        let isKept: Bool
        /// How far it is from the place asked about, metres, as the core
        /// says it.
        let distanceM: Double?
        /// Absent until the server has answered, and when it could not.
        var outlook: SiteOutlook?
        /// Whether the server has been asked and the asking is over,
        /// with an answer or without one.
        var isAsked = false

        var id: String { site.id }
    }

    typealias State = AskingState

    /// How long an answer is shown before the server is asked again.
    static let keepFor: TimeInterval = 30 * 60

    private(set) var state = State.idle
    /// The take-offs as the core lists them: the pilot's own first, then
    /// those around the place. Their order on a day is the core's too
    /// (`todayOrder`), asked by the screen for the day it shows.
    private(set) var places: [Place] = []
    /// What the core says with the list: what the two groups are called,
    /// the line about own take-offs that are not listed, and whose data
    /// the take-offs are.
    private(set) var listed: TodayTakeoffs?
    /// The date of the sites' data in the server's answer, for keeping
    /// one of them. Absent without an answer.
    private(set) var sitesAsOfUnixS: Int64?
    /// Why Venti gave no take-offs while the pilot's own are listed.
    private(set) var problem: String?
    /// Whose data the forecasts are. The licences ask for this next to
    /// the data.
    private(set) var credits: [Credit] = []
    /// Where the take-offs were looked for, when, and how much the pilot
    /// had kept and forgotten by then.
    private(set) var asked: (place: Coordinate, at: Date, changes: Int)?

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// The time by the server's clock, seconds since 1970-01-01 UTC: the
    /// hours that are gone are gone by that clock. The app sets this.
    @ObservationIgnored var clock: () async -> Int64 = { Int64(Date().timeIntervalSince1970) }
    /// How long to wait before asking again for a forecast the server is
    /// still fetching, and how often.
    @ObservationIgnored var forecastPause = Duration.seconds(5)
    static let forecastTries = 7
    /// Called when the take-offs have been asked about anew and every
    /// forecast that came is in: the home-screen widget's days are
    /// brought up to date then. The app sets this.
    @ObservationIgnored var onLoaded: () -> Void = {}

    @ObservationIgnored private let kept: KeptSitesModel
    @ObservationIgnored private let transport: Transport
    /// What was asked about single sites, for their own pages.
    @ObservationIgnored private var askedSites: [String: (outlook: SiteOutlook, at: Date)] = [:]

    init(kept: KeptSitesModel, transport: @escaping Transport = WindModel.fetch) {
        self.kept = kept
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

    /// Whether what is shown was asked for this place not long ago, and
    /// the pilot has kept or forgotten no site since.
    func isFresh(for place: Coordinate) -> Bool {
        guard let asked, state == .loaded, asked.changes == kept.changes else { return false }
        let moved = CLLocation(latitude: asked.place.lat, longitude: asked.place.lon)
            .distance(from: CLLocation(latitude: place.lat, longitude: place.lon))
        return moved < 5_000 && Date().timeIntervalSince(asked.at) < Self.keepFor
    }

    /// Asks for the take-offs around a place, has the core choose Today's
    /// among them and the pilot's own, and asks for the forecast of each.
    func load(around place: Coordinate) async {
        let changes = kept.changes
        let server = server()
        if places.isEmpty {
            state = .loading
        }
        // The sites around, from Venti. Without an answer the core lists
        // the pilot's own take-offs alone, which need no network.
        var near: SitesMap?
        var missing: String?
        if let server {
            do {
                near = try await sites(around: place, from: server)
            } catch is CancellationError {
                return
            } catch {
                if (error as? URLError)?.code == .cancelled {
                    return
                }
                missing = error is CoreError
                    ? error.words
                    : "Venti could not be reached for today's forecast. \(error.localizedDescription)"
            }
        } else {
            missing = "Venti could not be reached."
        }
        let found: TodayTakeoffs
        do {
            found = try await kept.todayTakeoffs(place: place, near: near)
        } catch {
            state = .failed(error.words)
            return
        }
        if let missing, found.takeoffs.isEmpty {
            if server == nil {
                places = []
                listed = nil
                credits = []
                asked = nil
            }
            state = .failed(missing)
            return
        }
        let nowUnixS = await clock()
        // The list at once, each forecast as it arrives. A take-off that
        // was listed before keeps what was read of it until then.
        let before = places
        places = found.takeoffs.map { takeoff in
            Place(
                site: takeoff.site, isKept: takeoff.isKept, distanceM: takeoff.distanceM,
                outlook: before.first { $0.id == takeoff.site.id }?.outlook
            )
        }
        listed = found
        sitesAsOfUnixS = near?.asOfUnixS
        problem = missing
        asked = (place, Date(), changes)
        state = .loaded
        guard let server else {
            for index in places.indices {
                places[index].isAsked = true
            }
            return
        }
        await withTaskGroup(of: (String, SiteOutlook?).self) { group in
            for takeoff in found.takeoffs {
                group.addTask { [weak self] in
                    (takeoff.site.id, await self?.outlook(for: takeoff.site, from: server, nowUnixS: nowUnixS))
                }
            }
            for await (id, outlook) in group {
                guard let index = places.firstIndex(where: { $0.id == id }) else { continue }
                places[index].isAsked = true
                guard let outlook else { continue }
                places[index].outlook = outlook
                if credits.isEmpty {
                    credits = outlook.credits
                }
            }
        }
        onLoaded()
    }

    /// The take-offs around a place as the core lists them for Today,
    /// the pilot's own first: an open plan's form offers them as its
    /// place. Empty where Venti gives none.
    func takeoffs(around place: Coordinate) async -> [TodayTakeoff] {
        guard let server = server(), let near = try? await sites(around: place, from: server) else { return [] }
        return (try? await kept.todayTakeoffs(place: place, near: near))?.takeoffs ?? []
    }

    /// The server's sites for the area the core says Today looks in.
    private func sites(around place: Coordinate, from server: URL) async throws -> SitesMap {
        let area = try todayArea(lat: place.lat, lon: place.lon)
        let path = try sitesPath(south: area.south, west: area.west, north: area.north, east: area.east)
        guard let url = URL(string: server.absoluteString + path) else { throw URLError(.badURL) }
        let (status, body) = try await transport(url)
        return try sitesMap(
            south: area.south, west: area.west, north: area.north, east: area.east,
            status: UInt16(clamping: status), body: body
        )
    }

    /// What the forecast says about a take-off. `nil` when no answer could
    /// be read.
    private func outlook(for site: SiteView, from server: URL, nowUnixS: Int64) async -> SiteOutlook? {
        guard let answer = await Self.forecastAnswer(
            for: site, from: server, transport: transport, tries: Self.forecastTries, pause: forecastPause
        ) else { return nil }
        return Self.outlook(of: site, in: answer, nowUnixS: nowUnixS)
    }

    /// The server's answer about the forecast at a take-off. The first
    /// forecast of a place is one the server has to fetch: it answers
    /// "not yet", and is asked again, `tries` times in all. `nil` when the
    /// server could not be asked.
    nonisolated static func forecastAnswer(
        for site: SiteView, from server: URL, transport: Transport, tries: Int, pause: Duration
    ) async -> (status: Int, body: Data)? {
        guard let path = try? forecastPath(lat: site.position.lat, lon: site.position.lon),
              let url = URL(string: server.absoluteString + path)
        else { return nil }
        var asked = 1
        while true {
            guard let (status, body) = try? await transport(url) else { return nil }
            if status == 503, asked < tries {
                asked += 1
                try? await Task.sleep(for: pause)
                continue
            }
            return (status, body)
        }
    }

    /// What the core reads in such an answer about the take-off, from a
    /// moment on. `nil` when it does not read.
    nonisolated static func outlook(
        of site: SiteView, in answer: (status: Int, body: Data), nowUnixS: Int64
    ) -> SiteOutlook? {
        let offset = TimeZone.current.secondsFromGMT(
            for: nowUnixS == 0 ? Date() : Date(timeIntervalSince1970: TimeInterval(nowUnixS))
        )
        return try? siteOutlook(
            status: UInt16(clamping: answer.status), body: answer.body,
            utcOffsetS: Int32(offset),
            wind: site.wind, elevationM: site.elevationM,
            nowUnixS: nowUnixS
        )
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

    /// How near the position is to be: to a kilometre for the take-offs
    /// around; `.best` for a call for help, which tells where to come.
    enum Accuracy {
        case kilometre, best
    }

    init(accuracy: Accuracy = .kilometre) {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = accuracy == .best ? kCLLocationAccuracyBest : kCLLocationAccuracyKilometer
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
