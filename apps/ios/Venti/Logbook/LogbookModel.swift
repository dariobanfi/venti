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
    /// How far an import has come.
    struct ImportProgress: Equatable, Sendable {
        var done: Int
        let total: Int
    }

    private(set) var view: LogbookView
    /// Set while files are being imported, for a screen to show.
    private(set) var importProgress: ImportProgress?
    /// The folder the core keeps its data in. The cockpit records into it.
    let folder: URL
    private let core: Core

    init(core: Core, folder: URL) {
        self.core = core
        self.folder = folder
        view = core.logbook()
    }

    /// Reads the logbook again, after something other than this model
    /// changed it: a sync with the server.
    func refresh() {
        let latest = core.logbook()
        if latest != view {
            view = latest
        }
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

    /// Imports flight files, and every flight file in the folders among
    /// them: a pilot's whole history at once. Reading and parsing run off
    /// the main thread; `importProgress` says how far it has come, and the
    /// list fills while it runs.
    func importFiles(at urls: [URL]) async -> ImportReport {
        guard importProgress == nil else {
            var report = ImportReport()
            report.failures = urls.map {
                ImportReport.Failure(
                    file: $0.lastPathComponent,
                    reason: "Another import is still running. Try again when it has finished."
                )
            }
            return report
        }
        importProgress = ImportProgress(done: 0, total: urls.count)
        defer { importProgress = nil }
        let core = self.core
        let report = await Task.detached(priority: .userInitiated) { [weak self] () -> ImportReport in
            // Files and folders picked in the Files app are only readable
            // inside this pair, and a folder's pair covers what is in it.
            let scoped = urls.filter { $0.startAccessingSecurityScopedResource() }
            defer {
                for url in scoped {
                    url.stopAccessingSecurityScopedResource()
                }
            }
            let files = LogbookModel.flightFiles(in: urls)
            var report = ImportReport()
            for (index, url) in files.enumerated() {
                do {
                    let imported = try core.importFlight(igcText: try LogbookModel.text(at: url))
                    if imported.alreadyInLogbook {
                        report.alreadyThere.append(imported.id)
                    } else {
                        report.added.append(imported.id)
                    }
                } catch {
                    report.failures.append(
                        ImportReport.Failure(file: url.lastPathComponent, reason: error.words)
                    )
                }
                // The count after every file; the list every so often.
                let done = index + 1
                let showsList = done % 20 == 0
                await self?.imported(done, of: files.count, showingList: showsList)
            }
            return report
        }.value
        view = core.logbook()
        return report
    }

    private func imported(_ done: Int, of total: Int, showingList: Bool) {
        importProgress = ImportProgress(done: done, total: total)
        if showingList {
            view = core.logbook()
        }
    }

    /// The files to import: those picked, and for a picked folder every
    /// flight file in it and in the folders inside it. A file picked by
    /// hand is tried whatever its name; in a folder only names ending in
    /// `.igc` count, so that a folder of photos is not a list of failures.
    nonisolated static func flightFiles(in urls: [URL]) -> [URL] {
        var files: [URL] = []
        for url in urls {
            let isFolder = (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory ?? false
            guard isFolder else {
                files.append(url)
                continue
            }
            let inside = FileManager.default.enumerator(
                at: url, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
            )
            var found: [URL] = []
            while let file = inside?.nextObject() as? URL {
                if file.pathExtension.lowercased() == "igc" {
                    found.append(file)
                }
            }
            // Recorders name files by date: oldest first reads naturally
            // in the list as it fills.
            files.append(contentsOf: found.sorted { $0.path < $1.path })
        }
        return files
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

    /// A flight checked against the airspace the app has. Runs off the
    /// main thread.
    func airspace(of id: String) async throws -> FlightAirspace {
        let core = self.core
        return try await Task.detached(priority: .userInitiated) {
            try core.flightAirspace(id: id)
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

    /// A new conversation with the assistant, which answers from this
    /// logbook.
    func makeAssistant(model: String) -> Assistant {
        core.assistant(model: model)
    }

    /// The logbook looked at as a whole: months, gliders, bests.
    func statistics() -> LogbookStatistics {
        core.statistics()
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
        // A batch says what it did; one new flight simply opens.
        if added.count > 1 || (!added.isEmpty && !alreadyThere.isEmpty) {
            let before = alreadyThere.isEmpty
                ? ""
                : alreadyThere.count == 1
                    ? " 1 was in your logbook already."
                    : " \(alreadyThere.count) were in your logbook already."
            return ImportNotice(
                title: added.count == 1 ? "1 flight added" : "\(added.count) flights added",
                text: (added.count == 1 ? "It is" : "They are") + " in your logbook, on this iPhone.\(before)"
            )
        }
        if added.isEmpty, alreadyThere.isEmpty {
            return ImportNotice(title: "No flights found", text: "There was no flight file among what you chose.")
        }
        return nil
    }
}

/// A message about an import, shown as an alert.
struct ImportNotice: Sendable, Equatable {
    let title: String
    let text: String
}
