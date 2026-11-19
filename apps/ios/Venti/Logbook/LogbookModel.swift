import Foundation
import Observation
import VentiCore

/// The pilot's logbook as the screens see it.
///
/// The model contains no flying logic. The core stores the flights, works out
/// every number and groups them; the model asks for the result and keeps the
/// latest copy for the screens to draw.
@MainActor
@Observable
final class LogbookModel {
    private(set) var view: LogbookView
    /// The folder the core keeps its data in. The cockpit records into it.
    let folder: URL
    private let core: Core

    init(core: Core, folder: URL) {
        self.core = core
        self.folder = folder
        view = core.logbook()
    }

    /// Opens the logbook kept in `folder`, creating it on first use.
    static func open(folder: URL) throws -> LogbookModel {
        LogbookModel(core: try Core(dataFolder: folder.path), folder: folder)
    }

    /// Moves a finished recording into the logbook and returns the flight's
    /// identifier. The recording is cleared only once the logbook has it,
    /// so a failure here loses nothing: it is found again at the next launch.
    func adopt(_ recording: PendingRecording) async throws -> String {
        let (core, folder) = (self.core, self.folder.path)
        let id = try await Task.detached(priority: .userInitiated) { () throws -> String in
            let imported = try core.importFlight(igcText: recording.igcText)
            try clearRecording(dataFolder: folder, name: recording.name)
            return imported.id
        }.value
        view = core.logbook()
        return id
    }

    /// Brings in recordings that were cut short: flights being recorded
    /// when the app was closed, or when it crashed. Called at launch.
    func adoptInterruptedRecordings() async {
        for recording in pendingRecordings(dataFolder: folder.path) {
            _ = try? await adopt(recording)
        }
    }

    /// Imports flight files. Reading and parsing run off the main thread.
    func importFiles(at urls: [URL]) async -> ImportReport {
        let core = self.core
        let report = await Task.detached(priority: .userInitiated) { () -> ImportReport in
            var report = ImportReport()
            for url in urls {
                // Files picked in the Files app are only readable inside this pair.
                let scoped = url.startAccessingSecurityScopedResource()
                defer {
                    if scoped {
                        url.stopAccessingSecurityScopedResource()
                    }
                }
                do {
                    let imported = try core.importFlight(igcText: try LogbookModel.text(at: url))
                    if imported.alreadyInLogbook {
                        report.alreadyThere.append(imported.id)
                    } else {
                        report.added.append(imported.id)
                    }
                } catch {
                    report.failures.append(
                        ImportReport.Failure(file: url.lastPathComponent, reason: error.localizedDescription)
                    )
                }
            }
            return report
        }.value
        view = core.logbook()
        return report
    }

    /// Imports the made-up flight bundled with the app, so a new pilot has
    /// something to look at.
    func addSample() async -> ImportReport {
        guard let url = Bundle.main.url(forResource: "sample", withExtension: "igc") else {
            var report = ImportReport()
            report.failures.append(
                ImportReport.Failure(file: "sample.igc", reason: "The sample flight is missing from the app.")
            )
            return report
        }
        return await importFiles(at: [url])
    }

    /// One flight, prepared for display. Runs off the main thread.
    func flight(id: String) async throws -> FlightView {
        let core = self.core
        return try await Task.detached(priority: .userInitiated) {
            try core.flight(id: id)
        }.value
    }

    /// What happened in one flight: thermals, glides, distance. Runs off
    /// the main thread.
    func analysis(id: String) async throws -> FlightAnalysis {
        let core = self.core
        return try await Task.detached(priority: .userInitiated) {
            try core.flightAnalysis(id: id)
        }.value
    }

    /// Writes a flight's file to a temporary folder, to be shared or saved
    /// elsewhere, and returns where it is.
    func exportFile(id: String) async throws -> URL {
        let core = self.core
        return try await Task.detached(priority: .userInitiated) { () throws -> URL in
            let file = try core.flightFile(id: id)
            let folder = FileManager.default.temporaryDirectory
                .appendingPathComponent("venti-export", isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let url = folder.appendingPathComponent(file.name)
            try file.igcText.write(to: url, atomically: true, encoding: .utf8)
            return url
        }.value
    }

    /// Removes a flight for good.
    func delete(id: String) throws {
        defer { view = core.logbook() }
        try core.deleteFlight(id: id)
    }

    /// The text of a flight file. They are plain ASCII by specification, but
    /// recorders write pilot names in whatever encoding they like.
    private nonisolated static func text(at url: URL) throws -> String {
        let data = try Data(contentsOf: url)
        return String(data: data, encoding: .utf8) ?? String(decoding: data, as: Unicode.ASCII.self)
    }
}

/// What happened to a batch of imported files.
struct ImportReport: Sendable, Equatable {
    struct Failure: Sendable, Equatable {
        let file: String
        let reason: String
    }

    /// Identifiers of the flights that were added.
    var added: [String] = []
    /// Identifiers of the flights the logbook already held.
    var alreadyThere: [String] = []
    var failures: [Failure] = []

    /// What the pilot should be told, if anything. A clean import needs no
    /// message: the new flights appear in the list.
    var notice: ImportNotice? {
        if !failures.isEmpty {
            let shown = failures.prefix(4).map { "\($0.file): \($0.reason)" }
            let more = failures.count > 4 ? ["and \(failures.count - 4) more."] : []
            return ImportNotice(
                title: failures.count == 1
                    ? "1 file could not be imported"
                    : "\(failures.count) files could not be imported",
                text: (shown + more).joined(separator: "\n")
            )
        }
        if added.isEmpty, !alreadyThere.isEmpty {
            return ImportNotice(
                title: "Already in your logbook",
                text: alreadyThere.count == 1
                    ? "This flight was imported before. Nothing was added."
                    : "These flights were imported before. Nothing was added."
            )
        }
        return nil
    }
}

/// A message about an import, shown as an alert.
struct ImportNotice: Sendable, Equatable {
    let title: String
    let text: String
}
