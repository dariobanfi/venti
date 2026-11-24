import Foundation
import Observation
import VentiCore

/// The pilot's task: the competition task of the day, or a route planned
/// for a cross-country flight. One is kept at a time.
///
/// Reading a task, measuring it and following a flight through it is the
/// core's. This class brings a task in from a file or the clipboard, and
/// hands it to the cockpit and to other pilots.
@MainActor
@Observable
final class TaskModel {
    /// The task that is kept, as screens show it.
    private(set) var task: TaskView?
    /// Why the last task could not be taken in, in words for the pilot.
    var problem: String?

    @ObservationIgnored private let core: Core

    /// No task file is larger than this; a larger file is something else.
    private static let largestFile = 256 * 1024

    init(core: Core) {
        self.core = core
        reload()
    }

    private static var now: Int64 {
        Int64(Date().timeIntervalSince1970)
    }

    /// Reads the kept task again: its times of day are those of today.
    func reload() {
        task = try? core.keptTask(nowUnixS: Self.now)
    }

    /// Keeps a task from the text of a file or of a QR code. It replaces
    /// the task kept before. Returns whether the text was a task.
    @discardableResult
    func keep(_ text: String) -> Bool {
        do {
            try core.keepTask(text: text)
            problem = nil
            reload()
            return true
        } catch {
            problem = error.words
            return false
        }
    }

    /// Keeps the task in a file the pilot chose.
    @discardableResult
    func keep(fileAt url: URL) -> Bool {
        let isScoped = url.startAccessingSecurityScopedResource()
        defer {
            if isScoped {
                url.stopAccessingSecurityScopedResource()
            }
        }
        let data: Data
        do {
            data = try Data(contentsOf: url)
        } catch {
            problem = "The file could not be read. \(error.localizedDescription)"
            return false
        }
        guard data.count <= Self.largestFile else {
            problem = "This file is too large to be a task."
            return false
        }
        return keep(String(decoding: data, as: UTF8.self))
    }

    /// Makes a route of these waypoints, in this order, and keeps it as
    /// the task. Returns whether that worked.
    @discardableResult
    func keepRoute(_ waypoints: [WaypointView]) -> Bool {
        do {
            try core.keepRoute(waypointIds: waypoints.map(\.id))
            problem = nil
            reload()
            return true
        } catch {
            problem = error.words
            return false
        }
    }

    /// Keeps a route drawn on the map as the task: its places in the
    /// order they were tapped.
    func keepDrawnRoute(_ places: [Coordinate]) -> Bool {
        do {
            try core.keepDrawnRoute(points: places)
            problem = nil
            reload()
            return true
        } catch {
            problem = error.words
            return false
        }
    }

    /// Forgets the kept task.
    func forget() {
        do {
            try core.forgetTask()
            problem = nil
        } catch {
            problem = error.words
        }
        reload()
    }

    /// The kept task as a file's text: for the cockpit, and for sharing.
    var file: TaskFile? {
        try? core.keptTaskFile()
    }

    /// The kept task as the text of a QR code.
    func code(compressed: Bool) -> String? {
        try? core.keptTaskCode(compressed: compressed)
    }

    /// Writes the kept task to a file that can be shared.
    func exportFile() throws -> URL? {
        guard let file else { return nil }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(file.name)
        try Data(file.text.utf8).write(to: url, options: .atomic)
        return url
    }
}

extension TaskView {
    /// The task in a line, for the Fly tab: "5 turnpoints · 45.4 km".
    var inALine: String {
        let count = turnpoints.count
        let places = isRoute ? "\(count) waypoints" : "\(count) turnpoints"
        return "\(places) · \(Format.kilometres(metres: lengthM)) km"
    }
}

extension TaskTurnpointView {
    /// What it is and how large, in a line: "Start · 5 km", "Goal line ·
    /// 400 m wide".
    var detail: String {
        guard radiusM > 0 else { return roleInWords }
        if role == .launch {
            return roleInWords
        }
        if isLine {
            return "\(roleInWords) · \(Self.length(2 * radiusM)) wide"
        }
        return "\(roleInWords) · \(Self.length(radiusM))"
    }

    /// A radius the way task boards write it: "400 m", "5 km", "2.5 km".
    static func length(_ metres: Double) -> String {
        if metres < 1000 {
            return "\(Format.metres(metres)) m"
        }
        let kilometres = metres / 1000
        let isWhole = kilometres == kilometres.rounded()
        return "\(Format.decimal(kilometres, places: isWhole ? 0 : 1)) km"
    }
}
