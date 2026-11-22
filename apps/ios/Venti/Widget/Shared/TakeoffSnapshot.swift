import AppIntents
import Foundation

/// What the home-screen widget "the next days at my take-off" is, for the
/// app and the widget alike.
enum TakeoffWidget {
    /// The widget's kind, by which the app asks the system to draw it anew.
    static let kind = "TakeoffDays"

    /// What a tap on the widget opens: the app, on Today.
    static let opensToday = URL(string: "venti://today")

    /// Whether an address handed to the app is the widget's.
    static func isTodaysAddress(_ url: URL) -> Bool {
        url.scheme == opensToday?.scheme && url.host() == opensToday?.host()
    }
}

/// What the app writes for the widget: the next days at each of the
/// pilot's own take-offs, in the core's own words and status values, and
/// when each was read.
///
/// The widget links no Rust and asks no server. It cannot ask the core
/// what a day is called tomorrow, so the app asks for it: an `Edition` is
/// the days as Today names and judges them from one moment on, and a
/// take-off has one for now and one for the day after. The widget shows
/// the edition whose time it is and never a day under an older name: the
/// day that was "Tomorrow" is "Today" in the next edition, and where there
/// is no next edition the widget says that the forecast is out of date.
struct TakeoffSnapshot: Codable, Equatable, Sendable {
    /// The core's `Flyability`, as a value the widget can read without
    /// the core.
    enum Verdict: String, Codable, Sendable {
        case flyable, marginal, notFlyable
    }

    /// A day at a take-off, as Today says it.
    struct Day: Codable, Equatable, Sendable, Identifiable {
        /// The day's first moment, seconds since 1970-01-01 UTC.
        var startUnixS: Int64
        /// What the core calls the day: "Today", "Tomorrow", "Fri".
        var name: String
        var verdict: Verdict
        /// The verdict in the core's word: "Flyable".
        var status: String
        /// The day in the core's line, which names the flying hours:
        /// "Good from 11:00 to 15:00".
        var line: String

        var id: Int64 { startUnixS }
    }

    /// The days as Today names and judges them from a moment on, until
    /// the moment their names no longer hold: the next day's beginning.
    struct Edition: Codable, Equatable, Sendable {
        var fromUnixS: Int64
        var untilUnixS: Int64
        var days: [Day]
    }

    /// One of the pilot's own take-offs.
    struct Takeoff: Codable, Equatable, Sendable, Identifiable {
        var id: String
        /// Its name as Today has it.
        var name: String
        /// When its forecast was last read, by the iPhone's clock; 0
        /// while none has been.
        var updatedUnixS: Int64
        /// The earliest first. Empty while no forecast has been read.
        var editions: [Edition]
    }

    /// The pilot's own take-offs as Today lists them without a place: the
    /// first is the widget's until the pilot chooses another.
    var takeoffs: [Takeoff] = []

    /// How long what was read of a take-off is shown: a day.
    static let goodForS: Int64 = 24 * 3600

    /// What the widget shows.
    enum Shown: Equatable, Sendable {
        /// The pilot keeps no take-off.
        case noTakeoff
        /// No forecast has been read for the take-off yet.
        case noForecast(name: String)
        /// What was read is more than a day old, or none of its days is
        /// left: said in place of the days.
        case outOfDate(name: String)
        case days(name: String, days: [Day], updatedUnixS: Int64)
    }

    /// The take-off the widget is set to; the first where it is set to
    /// none, or to one the pilot no longer keeps.
    func takeoff(_ id: String?) -> Takeoff? {
        takeoffs.first { $0.id == id } ?? takeoffs.first
    }

    /// What the widget shows of a take-off at a moment by the iPhone's
    /// clock.
    func shown(takeoff id: String?, nowUnixS: Int64) -> Shown {
        guard let takeoff = takeoff(id) else { return .noTakeoff }
        guard !takeoff.editions.isEmpty else { return .noForecast(name: takeoff.name) }
        guard nowUnixS - takeoff.updatedUnixS < Self.goodForS else { return .outOfDate(name: takeoff.name) }
        let edition = takeoff.editions.last { $0.fromUnixS <= nowUnixS } ?? takeoff.editions[0]
        guard nowUnixS < edition.untilUnixS, !edition.days.isEmpty else { return .outOfDate(name: takeoff.name) }
        return .days(name: takeoff.name, days: edition.days, updatedUnixS: takeoff.updatedUnixS)
    }

    /// The moments after one at which what is shown of a take-off
    /// changes: an edition begins or ends, or the day it is good for is
    /// over. The earliest first.
    func changes(takeoff id: String?, after nowUnixS: Int64) -> [Int64] {
        guard let takeoff = takeoff(id), !takeoff.editions.isEmpty else { return [] }
        let moments = takeoff.editions.flatMap { [$0.fromUnixS, $0.untilUnixS] }
            + [takeoff.updatedUnixS + Self.goodForS]
        return Set(moments.filter { $0 > nowUnixS }).sorted()
    }
}

/// Where the snapshot is kept: a file in the folder the app and the widget
/// share, written by the app and read by the widget.
struct TakeoffSnapshotStore: Sendable {
    let file: URL

    /// The file in the shared folder (the app group both targets'
    /// entitlements and Info.plists name). `nil` where the build has no
    /// such folder.
    static var shared: TakeoffSnapshotStore? {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "VentiAppGroup") as? String, !group.isEmpty,
              let folder = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
        else { return nil }
        return TakeoffSnapshotStore(file: folder.appendingPathComponent("takeoff-days.json"))
    }

    /// What was last written; `nil` where nothing was, or it does not read.
    func read() -> TakeoffSnapshot? {
        guard let data = try? Data(contentsOf: file) else { return nil }
        return try? JSONDecoder().decode(TakeoffSnapshot.self, from: data)
    }

    /// Writes the snapshot whole or not at all.
    func write(_ snapshot: TakeoffSnapshot) throws {
        try JSONEncoder().encode(snapshot).write(to: file, options: .atomic)
    }
}

/// The pilot's own take-offs for the widget's setup, read from what the app
/// wrote.
struct KeptTakeoffs: EntityQuery {
    func entities(for identifiers: [String]) async throws -> [KeptTakeoff] {
        all().filter { identifiers.contains($0.id) }
    }

    func suggestedEntities() async throws -> [KeptTakeoff] {
        all()
    }

    private func all() -> [KeptTakeoff] {
        (TakeoffSnapshotStore.shared?.read()?.takeoffs ?? []).map { KeptTakeoff(id: $0.id, name: $0.name) }
    }
}
