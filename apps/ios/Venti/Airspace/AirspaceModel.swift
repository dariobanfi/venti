import Foundation
import Observation
import VentiCore

/// The pilot's airspace as the screens see it.
///
/// The core keeps the files, reads them and answers every question about
/// them. This class asks for the results and reads files from where the
/// pilot picked them.
@MainActor
@Observable
final class AirspaceModel {
    /// The files the pilot has brought in.
    private(set) var files: [AirspaceFile] = []
    /// Every airspace of those files as one set, for the maps and the
    /// cockpit. Changes whenever the files do.
    private(set) var set: AirspaceSet?

    @ObservationIgnored private let core: Core

    init(core: Core) {
        self.core = core
        reload()
    }

    /// Whether there is any airspace to draw or to warn about.
    var isEmpty: Bool { files.isEmpty }

    /// The airspaces that touch a part of the map.
    func shapes(in area: Bounds) -> [AirspaceShape] {
        set?.inArea(area: area) ?? []
    }

    /// Takes in the chosen files and says what happened, in words.
    func importFiles(at urls: [URL]) async -> ImportNotice {
        let core = self.core
        let now = Int64(Date().timeIntervalSince1970)
        let lines = await Task.detached(priority: .userInitiated) { () -> [String] in
            var lines: [String] = []
            for url in urls {
                // Files picked in the Files app are only readable inside this pair.
                let scoped = url.startAccessingSecurityScopedResource()
                defer {
                    if scoped {
                        url.stopAccessingSecurityScopedResource()
                    }
                }
                do {
                    let result = try core.importAirspace(
                        fileName: url.lastPathComponent, bytes: Data(contentsOf: url), nowUnixS: now
                    )
                    lines.append(Self.summary(of: result, file: url.lastPathComponent))
                } catch {
                    lines.append("\(url.lastPathComponent): \(error.words)")
                }
            }
            return lines
        }.value
        reload()
        return ImportNotice(title: "Airspace", text: lines.joined(separator: "\n\n"))
    }

    func delete(_ file: AirspaceFile) {
        try? core.deleteAirspaceFile(storedName: file.storedName)
        reload()
    }

    private func reload() {
        files = (try? core.airspaceFiles()) ?? []
        set = files.isEmpty ? nil : try? core.airspace()
    }

    /// What one file gave, and above all what it did not.
    nonisolated static func summary(of result: AirspaceImport, file: String) -> String {
        var text = "\(file): \(count(result.airspaces)) read."
        if result.leftOut > 0 {
            text += " \(count(result.leftOut)) could not be read and \(result.leftOut == 1 ? "is" : "are") left out: you will not be warned about \(result.leftOut == 1 ? "it" : "them")."
        }
        if result.assumed > 0 {
            text += " In \(result.assumed == 1 ? "1 place" : "\(result.assumed) places") something could not be read and the cautious reading was taken."
        }
        if !result.problems.isEmpty {
            text += "\n" + result.problems.joined(separator: "\n")
        }
        return text
    }

    private nonisolated static func count(_ airspaces: UInt32) -> String {
        airspaces == 1 ? "1 airspace" : "\(airspaces.formatted()) airspaces"
    }
}
