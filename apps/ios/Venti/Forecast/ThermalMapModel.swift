import Foundation
import Observation
import VentiCore

/// The forecast map: the kind of day, square by square, over the area
/// Venti has forecasts for (`GET /v1/thermals`).
///
/// What to ask, how to read the answer and what each square says are the
/// core's (`thermalMapPath`, `thermalMap`). This class asks while the map
/// tab's layer is on, again once the answer is older than half an hour,
/// and five minutes after an asking that failed, as on Android. It keeps
/// the map it has when a later asking fails: a map fetched earlier in
/// this run of the app stays, with its run's time.
@MainActor
@Observable
final class ThermalMapModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    enum State: Equatable {
        /// Nothing asked yet, or no Venti to ask.
        case idle
        /// Asked, and no map yet.
        case loading
        /// There is a map.
        case loaded
        /// No map: refused or not reached, in words for the pilot.
        case failed(String)
    }

    /// How long a map is shown before Venti is asked again.
    static let keepFor: TimeInterval = 30 * 60
    /// How long after an asking that failed Venti is asked again by itself.
    static let retryAfter: TimeInterval = 5 * 60
    /// How often it is looked at, while the layer is on, whether the map
    /// has grown old.
    static let lookEvery = Duration.seconds(60)

    private(set) var state = State.idle
    /// The map, once there is one.
    private(set) var map: ThermalMap?
    /// When the map shown was fetched.
    private(set) var fetchedAt: Date?

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// The time by the server's clock, seconds since 1970-01-01 UTC: it
    /// decides which day is today. The app sets this.
    @ObservationIgnored var clock: () async -> Int64 = { Int64(Date().timeIntervalSince1970) }
    /// The iPhone's time, which the age of an answer is measured by.
    @ObservationIgnored var now: () -> Date = { Date() }

    @ObservationIgnored private let transport: Transport
    /// When Venti was last asked, whatever came of it, and which Venti.
    @ObservationIgnored private var askedAt: Date?
    @ObservationIgnored private var askedServer: URL?
    /// Whether the last asking failed.
    @ObservationIgnored private var hasFailed = false

    init(transport: @escaping Transport = WindModel.fetch) {
        self.transport = transport
    }

    /// Whether the map shown is this server's and younger than half an hour.
    var isFresh: Bool {
        guard map != nil, let fetchedAt, askedServer == server() else { return false }
        return now().timeIntervalSince(fetchedAt) < Self.keepFor
    }

    /// For as long as the layer is on: asks at once unless the map is
    /// fresh, and again whenever the last asking is half an hour old, or
    /// five minutes old where it failed. "Try again" asks at once.
    func keepCurrent() async {
        if !isFresh {
            await load()
        }
        while !Task.isCancelled {
            try? await Task.sleep(for: Self.lookEvery)
            if Task.isCancelled {
                return
            }
            if isDue {
                await load()
            }
        }
    }

    /// Whether it is time to ask again.
    var isDue: Bool {
        guard let askedAt, askedServer == server() else { return true }
        return now().timeIntervalSince(askedAt) >= (hasFailed ? Self.retryAfter : Self.keepFor)
    }

    /// Asks for the map.
    func load() async {
        guard let server = server() else {
            map = nil
            fetchedAt = nil
            askedAt = nil
            askedServer = nil
            state = .idle
            return
        }
        if askedServer != server {
            // Another server's map is not this one's.
            map = nil
            fetchedAt = nil
        }
        askedServer = server
        askedAt = now()
        if map == nil {
            state = .loading
        }
        do {
            guard let url = URL(string: server.absoluteString + thermalMapPath()) else {
                state = map == nil ? .failed("Venti's address cannot be asked for the forecast map.") : .loaded
                return
            }
            let (status, body) = try await transport(url)
            let nowUnixS = await clock()
            let offset = TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: TimeInterval(nowUnixS)))
            let read = try thermalMap(
                status: UInt16(clamping: status), body: body, utcOffsetS: Int32(offset), nowUnixS: nowUnixS
            )
            // The pilot may have changed the address while this was under way.
            guard server == self.server() else { return }
            map = read
            fetchedAt = now()
            hasFailed = false
            state = .loaded
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            guard server == self.server() else { return }
            hasFailed = true
            // A map fetched earlier stays, with its run's time.
            if map != nil {
                state = .loaded
            } else {
                state = .failed(
                    error is CoreError
                        ? error.words
                        : "Venti could not be reached for the forecast map. \(error.localizedDescription)"
                )
            }
        }
    }
}

/// What the map tab draws of the forecast map and which square is which,
/// worked out from the core's squares without a map.
enum ThermalSquares {
    /// One square as the map's fill layer draws it.
    struct Square: Equatable {
        /// Its place among the day's cells, to find it again when tapped.
        let index: Int
        /// The step of the pack's thermal scale, 1 to 6, or 0 for a day
        /// that is not flyable, which is filled in the danger colour.
        let fill: Int
        /// Its corners from the south-west, round to the first again.
        let ring: [Coordinate]
    }

    /// The fill of a square: the step the core names, 0 for a day that is
    /// not flyable, and `nil` where there is no estimate, which is not
    /// drawn.
    static func fill(of cell: ThermalMapCell) -> Int? {
        if let step = cell.scaleStep {
            return Int(step)
        }
        return cell.verdict == .notFlyable ? 0 : nil
    }

    /// The squares of a day that are drawn.
    static func squares(of day: ThermalMapDay) -> [Square] {
        day.cells.enumerated().compactMap { index, cell in
            guard let fill = fill(of: cell) else { return nil }
            return Square(
                index: index,
                fill: fill,
                ring: [
                    Coordinate(lat: cell.south, lon: cell.west),
                    Coordinate(lat: cell.north, lon: cell.west),
                    Coordinate(lat: cell.north, lon: cell.east),
                    Coordinate(lat: cell.south, lon: cell.east),
                    Coordinate(lat: cell.south, lon: cell.west),
                ]
            )
        }
    }

    /// The day the pilot chose, or the first the map has when that one is
    /// not among them (at first, and after midnight).
    static func day(_ date: String, in map: ThermalMap) -> ThermalMapDay? {
        map.days.first { $0.date == date } ?? map.days.first
    }

    /// Whether any square of the day that is drawn lies in the part of the
    /// map in view.
    static func anyDrawn(of day: ThermalMapDay, in view: Bounds) -> Bool {
        day.cells.contains { cell in
            fill(of: cell) != nil
                && cell.north > view.south && cell.south < view.north
                && cell.east > view.west && cell.west < view.east
        }
    }
}
