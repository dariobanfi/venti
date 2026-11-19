import Foundation
import Observation
import VentiCore

/// The pilot's waypoints as the screens see them.
///
/// The core keeps them, reads the files they come in and works out
/// distances. This class asks for the result and reads files from where
/// the pilot picked them.
@MainActor
@Observable
final class WaypointsModel {
    /// Every waypoint, by name.
    private(set) var all: [WaypointView] = []

    @ObservationIgnored private let core: Core

    init(core: Core) {
        self.core = core
        reload()
    }

    /// The waypoints nearest first from a place, each with its distance
    /// and direction; by name when the place is not known.
    func near(_ place: Coordinate?) -> [WaypointView] {
        (try? core.waypoints(from: place)) ?? []
    }

    /// Adds the waypoints of the chosen files and says what happened, in
    /// words for the pilot.
    func importFiles(at urls: [URL]) async -> ImportNotice {
        let core = self.core
        let (added, known, problems) = await Task.detached(priority: .userInitiated) {
            () -> (UInt32, UInt32, [String]) in
            var (added, known): (UInt32, UInt32) = (0, 0)
            var problems: [String] = []
            for url in urls {
                // Files picked in the Files app are only readable inside this pair.
                let scoped = url.startAccessingSecurityScopedResource()
                defer {
                    if scoped {
                        url.stopAccessingSecurityScopedResource()
                    }
                }
                do {
                    let result = try core.importWaypoints(text: Self.text(at: url))
                    added += result.added
                    known += result.alreadyThere
                    if result.skippedLines > 0 {
                        let lines = result.skippedLines == 1 ? "1 line" : "\(result.skippedLines) lines"
                        problems.append(
                            "\(url.lastPathComponent): \(lines) could not be read. \(result.firstProblem ?? "")"
                        )
                    }
                } catch {
                    problems.append("\(url.lastPathComponent): \(error.words)")
                }
            }
            return (added, known, problems)
        }.value
        reload()
        return Self.notice(added: added, known: known, problems: problems)
    }

    /// Adds one waypoint: a flying site from the map, for example.
    func add(name: String, position: Coordinate, elevationM: Double?) throws {
        defer { reload() }
        _ = try core.addWaypoint(name: name, position: position, elevationM: elevationM)
    }

    func delete(id: String) {
        try? core.deleteWaypoint(id: id)
        reload()
    }

    func deleteAll() {
        try? core.deleteAllWaypoints()
        reload()
    }

    /// Writes the waypoints to a temporary file, to share it.
    func exportFile() throws -> URL {
        let file = try core.waypointsFile()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(file.name)
        try file.text.write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    private func reload() {
        all = (try? core.waypoints(from: nil)) ?? []
    }

    /// What an import did, as a title and a text.
    nonisolated static func notice(added: UInt32, known: UInt32, problems: [String]) -> ImportNotice {
        let title: String =
            switch (added, known) {
            case (0, 0): "No waypoints added"
            case (0, _): "Nothing new"
            case (1, _): "1 waypoint added"
            default: "\(added) waypoints added"
            }
        var lines: [String] = []
        if known > 0 {
            lines.append(known == 1 ? "1 was already there." : "\(known) were already there.")
        }
        lines.append(contentsOf: problems)
        return ImportNotice(title: title, text: lines.joined(separator: "\n"))
    }

    /// The text of a waypoint file: UTF-8, or the Windows encoding older
    /// programs write.
    private nonisolated static func text(at url: URL) throws -> String {
        let data = try Data(contentsOf: url)
        return String(data: data, encoding: .utf8)
            ?? String(data: data, encoding: .windowsCP1252)
            ?? String(decoding: data, as: UTF8.self)
    }
}

extension WaypointView {
    /// The symbol for what the waypoint is.
    var symbolName: String {
        switch kind {
        case .takeoff: "arrow.up.right.circle"
        case .landing: "arrow.down.to.line.circle"
        case .other: "mappin.circle"
        }
    }

    /// The waypoint as a place for the cockpit to fly to.
    var place: TargetPlace {
        TargetPlace(name: name, position: position, elevationM: elevationM)
    }

    /// Its height, and how far and which way it is when that is known.
    var detail: String {
        var parts: [String] = []
        if let distanceM, let bearingDeg {
            parts.append("\(Format.kilometres(metres: distanceM)) km \(Format.compassPoint(degrees: bearingDeg))")
        }
        if let elevationM {
            parts.append("\(Format.metres(elevationM)) m")
        }
        if !code.isEmpty, code != name {
            parts.append(code)
        }
        return parts.joined(separator: " · ")
    }
}
