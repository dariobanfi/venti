import Foundation
import Observation
import VentiCore

/// What only a server that sees many gliders can say: where pilots climb
/// at this moment, where the public flights' thermals have been, and how
/// many pilots are in the air at a site. None of it names a pilot.
///
/// What to ask and how to read the answer is the core's (`live_air.rs`);
/// this class sends the requests and keeps the last answers. Both of the
/// map's layers are off until the pilot switches them on, and nothing is
/// asked while they are off.
@MainActor
@Observable
final class AirModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// How often the climbs are asked for again while their layer is on,
    /// seconds, as the core asks.
    static let climbsEvery = 30.0
    /// How long the map rests before it is asked about, milliseconds.
    static let restMilliseconds = 400
    /// The box asked about around a site, degrees to each side: a few
    /// kilometres, well inside what the server answers for.
    static let aroundASiteDeg = 0.05

    /// The climbs of the last minutes in the part of the map last asked
    /// for; `nil` while none was read.
    private(set) var climbs: LiveClimbs?
    /// Whether the map shows too much for the climbs to be asked for.
    private(set) var climbsTooWide = false
    /// Where thermals have been in the part of the map last asked for;
    /// `nil` while none was read.
    private(set) var flown: FlownThermals?
    /// Whether the map shows too much for them: the map then says the
    /// core's `flownTooWideWords()`.
    private(set) var flownTooWide = false

    /// The server to ask. The app sets this.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let transport: Transport

    /// By default nothing is taken from a cache: a climb that is kept is
    /// a wrong one.
    init(transport: @escaping Transport = LiveModel.fetch) {
        self.transport = transport
    }

    /// Asks for the climbs in the part of the map in view; with the layer
    /// off, or no part of the map, forgets them. An answer that does not
    /// come, or a server from before the climbs, leaves none: a climb is
    /// of this moment, and an old one is not shown as one.
    func loadClimbs(_ region: Bounds?, isOn: Bool) async {
        let path = region.flatMap {
            liveClimbsPath(south: $0.south, west: $0.west, north: $0.north, east: $0.east)
        }
        let tooWide = isOn && region != nil && path == nil
        if tooWide != climbsTooWide {
            climbsTooWide = tooWide
        }
        guard isOn, let server = server(), let path else {
            if climbs != nil {
                climbs = nil
            }
            return
        }
        let read = await ask(server, path) { status, body in try liveClimbs(status: status, body: body) }
        guard !Task.isCancelled else { return }
        if read != climbs {
            climbs = read
        }
    }

    /// Asks where thermals have been in the part of the map in view, in
    /// every month of the year and at any hour: the layer is not empty
    /// in winter, and the core's note says so. With the layer off, or no
    /// part of the map, forgets them.
    func loadFlown(_ region: Bounds?, isOn: Bool) async {
        let path = region.flatMap {
            flownThermalsPath(
                south: $0.south, west: $0.west, north: $0.north, east: $0.east,
                month: nil, fromHour: nil, toHour: nil
            )
        }
        let tooWide = isOn && region != nil && path == nil
        if tooWide != flownTooWide {
            flownTooWide = tooWide
        }
        guard isOn, let server = server(), let path else {
            if flown != nil {
                flown = nil
            }
            return
        }
        // What was there stays while the server does not answer: the past
        // does not change by the minute.
        let read = await ask(server, path) { status, body in try flownThermals(status: status, body: body) }
        guard !Task.isCancelled, let read, read != flown else { return }
        flown = read
    }

    /// What a site's page says of who flies there now: the core's line
    /// and the credits of the networks that counted.
    struct SiteLine: Equatable {
        var words = ""
        var credits: [Credit] = []
    }

    /// The line a site's page says of who flies there now, in the core's
    /// words; empty when nobody does, when there is no server, and when
    /// it does not answer. `name` is the site's own name, or empty for a
    /// site without one: the core then says "here".
    func siteAir(id: String, name: String, position: Coordinate) async -> SiteLine {
        let around = Self.aroundASiteDeg
        guard let server = server(),
              let path = liveSitesPath(
                  south: position.lat - around, west: position.lon - around,
                  north: position.lat + around, east: position.lon + around
              ),
              let air = await ask(server, path, read: { status, body in try liveSites(status: status, body: body) })
        else { return SiteLine() }
        let words = siteAirWords(air: air, siteId: id, siteName: name)
        return SiteLine(words: words, credits: words.isEmpty ? [] : air.credits)
    }

    /// The climbs around a glider, for the cockpit: the core's path and
    /// its reading of the answer. `nil` when they cannot be read.
    func climbsAround(_ place: Coordinate) async -> LiveClimbs? {
        guard let server = server(), let path = cockpitClimbsPath(lat: place.lat, lon: place.lon) else {
            return nil
        }
        return await ask(server, path) { status, body in try liveClimbs(status: status, body: body) }
    }

    private func ask<Answer>(
        _ server: URL, _ path: String, read: (UInt16, Data) throws -> Answer
    ) async -> Answer? {
        guard let url = URL(string: server.absoluteString + path),
              let (status, body) = try? await transport(url)
        else { return nil }
        return try? read(UInt16(clamping: status), body)
    }
}
