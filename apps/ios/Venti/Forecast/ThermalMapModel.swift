import Foundation
import Observation
import VentiCore

/// How a model's asking of Venti stands: the forecast map's and its
/// hours', Today's and foehn's.
enum AskingState: Equatable {
    /// Nothing asked yet, or no Venti to ask.
    case idle
    /// Asked, and no answer yet.
    case loading
    /// The answer is there.
    case loaded
    /// Refused or not reached, in words for the pilot.
    case failed(String)
}

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

    /// The map's: loaded once there is a map.
    typealias State = AskingState

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

    /// The map's answer as it came, which a tapped square's days are read
    /// from again (`thermalMapStrip`).
    @ObservationIgnored private var answer: (status: UInt16, body: Data)?

    // MARK: The hours of a day

    /// A layer of a day that can be asked for: the day's date, the layer,
    /// and for the wind the height above the sea, none at the ground.
    struct HoursKey: Hashable {
        let date: String
        let layer: ThermalHoursLayer
        let heightM: Int32?
        /// For the wind, the height above the model's ground: 250.
        var aboveGroundM: Int32? = nil
    }

    /// The hours': idle while no layer of hours is asked for, and the map
    /// shows the day, as it does when they failed.
    typealias HoursState = AskingState

    private(set) var hoursState = HoursState.idle
    /// The hours of the layer asked for last, once they are there.
    private(set) var hours: ThermalMapHours?
    /// What `hours` are of.
    private(set) var hoursKey: HoursKey?
    /// The hours of every day of the map for the layer of the line of
    /// time, by the day's date, as far as they are there.
    private(set) var line: [String: ThermalMapHoursView] = [:]
    /// What `line` is of.
    private(set) var lineKey: LineKey?

    /// The answers of this run of the app, by the address asked, for the
    /// map of the days they belong to: the wind at the ground and the
    /// gusts are one answer, read twice.
    @ObservationIgnored private var bodies: [String: (status: UInt16, body: Data)] = [:]
    /// The answers as the core read them, by what was asked.
    @ObservationIgnored private var read: [HoursKey: ThermalMapHours] = [:]
    /// Which map of the days the answers kept belong to: its run and when
    /// it was worked out.
    @ObservationIgnored private var keptFor: (run: Int64, computed: Int64)?

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

    /// The thermal forecast by region over the same days, asked of the
    /// same server by the same clock.
    let regions: RegionForecastModel

    init(transport: @escaping Transport = WindModel.fetch) {
        self.transport = transport
        regions = RegionForecastModel(transport: transport)
        regions.server = { [weak self] in self?.server() }
        regions.clock = { [weak self] in await self?.clock() ?? Int64(Date().timeIntervalSince1970) }
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
                state = map == nil ? .failed("Venti could not be asked for the forecast map.") : .loaded
                return
            }
            let (status, body) = try await transport(url)
            let nowUnixS = await clock()
            let offset = TimeZone.current.secondsFromGMT(for: nowUnixS == 0 ? Date() : Date(timeIntervalSince1970: TimeInterval(nowUnixS)))
            let read = try thermalMap(
                status: UInt16(clamping: status), body: body, utcOffsetS: Int32(offset), nowUnixS: nowUnixS
            )
            // The pilot may have changed the address while this was under way.
            guard server == self.server() else { return }
            if keptFor.map({ $0.run != read.runUnixS || $0.computed != read.computedUnixS }) ?? true {
                // A new map of the days: the hours kept were of the old.
                forgetHours()
                keptFor = (read.runUnixS, read.computedUnixS)
            }
            answer = (UInt16(clamping: status), body)
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

extension ThermalMapModel {
    /// The time and the offset of the iPhone's clock from UTC by which
    /// the days are cut, as the map of the days was read.
    private func clockAndOffset() async -> (now: Int64, offset: Int32) {
        let nowUnixS = await clock()
        let offset = TimeZone.current.secondsFromGMT(for: nowUnixS == 0 ? Date() : Date(timeIntervalSince1970: TimeInterval(nowUnixS)))
        return (nowUnixS, Int32(offset))
    }

    /// Shows a layer of a day hour by hour, or the day again for `nil`.
    /// An answer of this run of the app for the map of the days at hand is
    /// shown without asking; otherwise the layer before is taken away
    /// while Venti is asked.
    func showHours(_ key: HoursKey?) async {
        hoursKey = key
        guard let key else {
            hours = nil
            hoursState = .idle
            return
        }
        if let kept = read[key] {
            hours = kept
            hoursState = .loaded
            return
        }
        hours = nil
        do {
            let hours = try await self.hours(of: key) { [weak self] in
                if self?.hoursKey == key {
                    self?.hoursState = .loading
                }
            }
            // Something else may have been asked for meanwhile.
            guard hoursKey == key else { return }
            self.hours = hours
            hoursState = .loaded
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            guard hoursKey == key else { return }
            hoursState = .failed(
                error is CoreError
                    ? error.words
                    : "Venti could not be reached for the hours of this layer. \(error.localizedDescription)"
            )
        }
    }

    /// The hours of a layer of a day: read again from an answer of this
    /// run of the app, else asked for (`asking` is called first).
    private func hours(of key: HoursKey, asking: () -> Void = {}) async throws -> ThermalMapHours {
        if let kept = read[key] {
            return kept
        }
        let path = try thermalHoursPath(
            date: key.date, layer: key.layer, heightM: key.heightM, aboveGroundM: key.aboveGroundM
        )
        let answer: (status: UInt16, body: Data)
        if let kept = bodies[path] {
            answer = kept
        } else {
            guard let server = server(), let url = URL(string: server.absoluteString + path) else {
                throw URLError(.badURL)
            }
            asking()
            let (status, body) = try await transport(url)
            answer = (UInt16(clamping: status), body)
        }
        let (nowUnixS, offset) = await clockAndOffset()
        let hours = try ThermalMapHours.read(
            status: answer.status, body: answer.body, layer: key.layer, utcOffsetS: offset, nowUnixS: nowUnixS
        )
        bodies[path] = answer
        read[key] = hours
        return hours
    }

    /// What the map's one line of time is of: a layer at a height, for a
    /// map of the days.
    struct LineKey: Hashable {
        let layer: ThermalHoursLayer
        let heightM: Int32?
        var aboveGroundM: Int32? = nil
        let computed: Int64
    }

    /// Asks for the hours of every day of the map for one layer, for the
    /// map tab's one line of time, the first day first; each day's hours
    /// join `line` as they come. A day that cannot be had is left out.
    func loadLine(layer: ThermalHoursLayer, heightM: Int32?, aboveGroundM: Int32? = nil) async {
        guard let map else { return }
        let key = LineKey(layer: layer, heightM: heightM, aboveGroundM: aboveGroundM, computed: map.computedUnixS)
        if lineKey != key {
            lineKey = key
            line = [:]
        }
        for day in map.days where line[day.date] == nil {
            let hours = try? await hours(
                of: HoursKey(date: day.date, layer: layer, heightM: heightM, aboveGroundM: aboveGroundM)
            )
            if Task.isCancelled || lineKey != key {
                return
            }
            if let hours {
                line[day.date] = hours.view()
            }
        }
    }

    /// Every day of the map for the square at a place: its name, and the
    /// square's verdict and kind of day. Empty without a map.
    func strip(at place: Coordinate) async -> [ThermalStripDay] {
        guard let answer else { return [] }
        let (nowUnixS, offset) = await clockAndOffset()
        return (try? thermalMapStrip(
            status: answer.status, body: answer.body, utcOffsetS: offset, nowUnixS: nowUnixS,
            lat: place.lat, lon: place.lon
        )) ?? []
    }

    /// Forgets the hours kept: they were of another map of the days.
    private func forgetHours() {
        bodies = [:]
        read = [:]
        regions.forget()
        hours = nil
        line = [:]
        lineKey = nil
        if hoursState != .idle {
            hoursState = .idle
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
        /// that is not flyable, which is filled as a quiet veil.
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
