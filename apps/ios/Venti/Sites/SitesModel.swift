import Foundation
import Observation
import VentiCore

/// The flying sites for the map, from the Venti server the pilot has set:
/// take-offs, landings, winch sites and training slopes.
///
/// What to ask the server and how to read its answer is the core's. This
/// class sends the request and keeps the answer for the part of the map in
/// view. Sites change by the day, not by the minute, so an answer is kept
/// for an hour.
@MainActor
@Observable
final class SitesModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// How long an answer is used before the server is asked again.
    static let keepFor: TimeInterval = 60 * 60

    /// A map that shows more than this many degrees of longitude draws no
    /// sites: at that scale they are a carpet of dots. It is about the
    /// width of a phone's map at zoom 9.
    static let widestMap = 0.8

    /// The sites around the part of the map in view.
    private(set) var sites: [SiteView] = []
    /// Whose data they are. The licences ask for this next to the data.
    private(set) var credits: [Credit] = []
    /// Whether the server holds everything its sources have for the area.
    /// If not, a map without sites says nothing.
    private(set) var isWhole = false
    /// The day of the oldest source's data, seconds since 1970-01-01 UTC:
    /// a site added to the source since then is not here yet.
    private(set) var asOfUnixS: Int64?
    /// Why the sites are missing, in words for the pilot.
    private(set) var problem: String?

    /// What the pilot asks of the sites, on the map and in the list: kept
    /// between launches where the model was given a place to keep it.
    private(set) var choice: SitesChoice
    /// Where the wind comes from that the wind station nearest to the
    /// map's middle measured last, degrees: the direction of "Measured
    /// now". Absent where no station near has a current one.
    private(set) var measuredFromDeg: Double?
    /// What the filter leaves of `sites`, with the core's words for it:
    /// the sites that fit, and apart from them the take-offs whose source
    /// does not say. The rule is the core's (`filterSites`); it is asked
    /// when the sites, the choice or the wind change, not with every
    /// drawing of a screen.
    private(set) var filtered: SitesFiltered

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let transport: Transport
    /// Fetches around the system's cache: for asking again after a pilot
    /// wrote a site, when the answer kept is known to be old.
    @ObservationIgnored private let fresh: Transport
    /// The area the sites were asked for, from which server, and when.
    @ObservationIgnored private var answered: (area: Bounds, server: URL, at: Date)?
    @ObservationIgnored private var waiting: Task<Void, Never>?
    /// Where the pilot's choice is kept between launches, if anywhere.
    @ObservationIgnored private let store: UserDefaults?

    /// - Parameter store: where the filter is kept as the pilot left it:
    ///   the app's preferences. Without one, as in a unit test, it starts
    ///   asking nothing and is kept nowhere.
    init(
        transport: @escaping Transport = WindModel.fetch, fresh: @escaping Transport = SiteEditing.fresh,
        store: UserDefaults? = nil
    ) {
        self.transport = transport
        self.fresh = fresh
        self.store = store
        let choice = store.map(SitesChoice.init(keptIn:)) ?? SitesChoice()
        self.choice = choice
        filtered = filterSites(sites: [], filter: choice.filter(measuredFromDeg: nil))
    }

    /// The pilot asks something else of the sites.
    func choose(_ choice: SitesChoice) {
        guard choice != self.choice else { return }
        self.choice = choice
        if let store {
            choice.keep(in: store)
        }
        refilter()
    }

    /// The map says which wind its stations measured around its middle.
    /// It changes what is shown only while the pilot asks for the wind
    /// measured now.
    func measured(fromDeg: Double?) {
        guard fromDeg != measuredFromDeg else { return }
        measuredFromDeg = fromDeg
        if choice.wind == .measuredNow {
            refilter()
        }
    }

    /// The direction of the wind measured now around a place: that of the
    /// nearest station, by the core's order, whose reading is current and
    /// has a direction.
    nonisolated static func measuredNow(stations: [WindStation], around place: Coordinate) -> Double? {
        stationsNear(
            stations: stations, place: place, elevationM: nil, wind: [],
            withinM: .greatestFiniteMagnitude, atMost: UInt32(clamping: stations.count)
        )
        .lazy
        .filter { $0.station.mark.symbol != .old }
        .compactMap { $0.station.latest?.directionDeg }
        .first
    }

    /// Asks the core what the filter leaves of the sites.
    private func refilter() {
        filtered = filterSites(sites: sites, filter: choice.filter(measuredFromDeg: measuredFromDeg))
    }

    /// The map says which part of it is in view. The server is asked
    /// unless that part is covered by an answer still in use, or the map
    /// shows too much for sites to be drawn.
    func show(_ region: Bounds) {
        guard let server = server() else {
            forget()
            return
        }
        guard region.east - region.west <= Self.widestMap else { return }
        if let answered, answered.server == server, answered.area.contains(region),
           Date().timeIntervalSince(answered.at) < Self.keepFor
        {
            return
        }
        waiting?.cancel()
        waiting = Task { [weak self] in
            // The map reports every pause of a drag; wait until it rests.
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            // More than is in view, so that a drag needs no new request.
            await self?.load(region.grown(by: 0.5), from: server)
        }
    }

    /// Asks the server for the sites of an area.
    func load(_ area: Bounds, from server: URL) async {
        await load(area, from: server, with: transport)
    }

    private func load(_ area: Bounds, from server: URL, with transport: Transport) async {
        do {
            let path = try sitesPath(south: area.south, west: area.west, north: area.north, east: area.east)
            guard let url = URL(string: server.absoluteString + path) else { return }
            let (status, body) = try await transport(url)
            let map = try sitesMap(
                south: area.south, west: area.west, north: area.north, east: area.east,
                status: UInt16(clamping: status), body: body
            )
            // The pilot may have changed the server while this was under way.
            guard server == self.server() else { return }
            sites = map.sites
            credits = map.credits
            isWhole = map.isWhole
            asOfUnixS = map.asOfUnixS
            answered = (area, server, Date())
            problem = nil
            refilter()
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = error is CoreError
                ? error.words
                : "The server could not be reached for flying sites. \(error.localizedDescription)"
        }
    }

    /// Asks again for the area last asked for, whatever the age of its
    /// answer: a pilot added, corrected or removed a site.
    func refresh() async {
        guard let answered else { return }
        waiting?.cancel()
        await load(answered.area, from: answered.server, with: fresh)
    }

    private func forget() {
        waiting?.cancel()
        if !sites.isEmpty || answered != nil {
            sites = []
            credits = []
            isWhole = false
            asOfUnixS = nil
            answered = nil
            refilter()
        }
        problem = nil
    }
}

extension SiteKind {
    /// The design language's icon a site of this kind is shown with, in
    /// lists and sheets. The pack has one for taking off, one for landing
    /// and one for a site as such.
    var icon: String {
        switch self {
        case .takeoff, .towing: "icon-takeoff"
        case .landing: "icon-landing"
        case .training, .other: "icon-site"
        }
    }

    /// The kind in the name of its picture on the map.
    var pictureName: String {
        switch self {
        case .takeoff: "site-takeoff"
        case .landing: "site-landing"
        case .towing: "site-towing"
        case .training: "site-training"
        case .other: "site-other"
        }
    }

    /// Every kind, for making the map's pictures.
    static var all: [SiteKind] { [.takeoff, .landing, .towing, .training, .other] }
}
