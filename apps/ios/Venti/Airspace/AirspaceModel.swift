import Foundation
import Observation
import VentiCore

/// The pilot's airspace as the screens see it.
///
/// It comes from two places: files the pilot brings in, and the pilot's
/// Venti server, which the app asks for the airspace around where the
/// pilot is and whose answer it keeps on the iPhone. The core keeps both,
/// reads them and answers every question about them. This class asks for
/// the results, reads files from where the pilot picked them and sends the
/// request to the server.
@MainActor
@Observable
final class AirspaceModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// After a request that failed, the server is left alone for this long.
    static let retryAfter: TimeInterval = 5 * 60

    /// The files the pilot has brought in.
    private(set) var files: [AirspaceFile] = []
    /// The airspace kept from the server, one entry for each source.
    private(set) var kept: [KeptAirspace] = []
    /// Every airspace of the files and of what is kept as one set, for the
    /// maps and the cockpit. Changes whenever either does.
    private(set) var set: AirspaceSet?
    /// Set when there is airspace but it could not be put together: the
    /// cockpit would be silent, and the pilot must know.
    private(set) var problem: String?
    /// Why the server's airspace could not be fetched, in words.
    private(set) var serverProblem: String?
    /// Whether the server is asked for airspace at all. The pilot's
    /// choice, remembered.
    var usesServer = true {
        didSet {
            if remembersChoice {
                UserDefaults.standard.set(!usesServer, forKey: Preferences.airspaceWithoutServer)
            }
        }
    }

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }

    @ObservationIgnored private let core: Core
    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let remembersChoice: Bool
    @ObservationIgnored private var isFetching = false
    @ObservationIgnored private var failedAt: Date?
    /// `retryAfter`, which a test makes short.
    @ObservationIgnored var leftAloneFor = AirspaceModel.retryAfter

    /// - Parameter remembersChoice: whether the choice to use the server
    ///   is read from and saved to the settings. Tests do neither.
    init(core: Core, transport: @escaping Transport = WindModel.fetch, remembersChoice: Bool = true) {
        self.core = core
        self.transport = transport
        self.remembersChoice = remembersChoice
        if remembersChoice, UserDefaults.standard.bool(forKey: Preferences.airspaceWithoutServer) {
            usesServer = false
        }
        reload()
    }

    /// Whether there is any airspace to draw or to warn about.
    var isEmpty: Bool { files.isEmpty && kept.isEmpty }

    /// Whose airspace the server's is, with what their licences ask to be
    /// shown wherever it is.
    var credits: [Credit] { kept.compactMap(\.credit) }

    /// What the cockpit will warn from, in a line for before take-off.
    var inALine: String {
        var parts = kept.map { "\($0.provider), \(Self.count($0.airspaces))" }
        if !files.isEmpty {
            parts.append(files.count == 1 ? "1 file" : "\(files.count) files")
        }
        return parts.isEmpty ? "None" : parts.joined(separator: " · ")
    }

    /// The airspaces that touch a part of the map, leaving off those that
    /// certainly begin above `ceilingM` when one is chosen (0 or less is
    /// every height, as the preference keeps it).
    func shapes(in area: Bounds, below ceilingM: Double = 0) -> [AirspaceShape] {
        set?.inAreaBelow(area: area, ceilingM: ceilingM > 0 ? ceilingM : nil) ?? []
    }

    /// What lies over a place, from the lowest floor up, in the core's
    /// words. `nil` before the app has its airspace.
    func over(_ place: Coordinate) -> AirspaceOverPlace? {
        set?.overPlace(place: place)
    }

    /// Takes in the chosen files and says what happened, in words.
    func importFiles(at urls: [URL]) async -> ImportNotice {
        let core = self.core
        let now = Int64(Date().timeIntervalSince1970)
        let lines = await Task.detached(priority: .userInitiated) { () -> [String] in
            var lines: [String] = []
            for url in urls {
                let endAccess = url.startAccess()
                defer { endAccess() }
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

    /// Asks the server for the airspace around a place and keeps it, when
    /// the core says it is time: nothing is kept, what is kept is a day
    /// old, or the place is at the edge of it or beyond. Otherwise nothing
    /// is sent. Called whenever the app learns where the pilot is looking
    /// or flying.
    func refresh(around place: Coordinate) async {
        guard usesServer, !isFetching, let address = server()?.absoluteString else { return }
        if let failedAt, Date().timeIntervalSince(failedAt) < leftAloneFor {
            return
        }
        let now = Int64(Date().timeIntervalSince1970)
        guard core.keptAirspaceIsDue(server: address, lat: place.lat, lon: place.lon, nowUnixS: now) else { return }
        isFetching = true
        defer { isFetching = false }
        let (lat, lon) = (place.lat, place.lon)
        do {
            let path = try serverAirspacePath(lat: lat, lon: lon)
            guard let url = URL(string: address + path) else { return }
            let (status, body) = try await transport(url)
            let core = self.core
            // A country's airspace is a megabyte to read and to write.
            _ = try await Task.detached(priority: .utility) {
                try core.keepServerAirspace(
                    server: address, lat: lat, lon: lon,
                    status: UInt16(clamping: status), body: body, nowUnixS: now
                )
            }.value
            failedAt = nil
            serverProblem = nil
            reload()
        } catch {
            // A task that was cancelled asked nobody: its caller asks anew
            // (the Fly tab's task is cancelled when the position it is
            // named by comes in). Taken for a failure, it kept the server
            // from being asked for five minutes.
            if error is CancellationError || (error as? URLError)?.code == .cancelled {
                return
            }
            failedAt = Date()
            serverProblem = error is CoreError
                ? error.words
                : "Venti could not be reached for airspace. \(error.localizedDescription)"
        }
    }

    /// The same for the Fly tab and the cockpit, which need the airspace
    /// before a flight and in it whatever the map tab draws: asked now,
    /// and where the server could not be asked, again after `retryAfter`
    /// for as long as the caller's task runs. A pilot who stands at the
    /// take-off while the network comes and goes has the airspace once it
    /// came, not only after flying on.
    func keepCovered(around place: Coordinate) async {
        while !Task.isCancelled {
            await refresh(around: place)
            guard usesServer, serverProblem != nil else { return }
            do {
                // A moment past the time the server is left alone for.
                try await Task.sleep(for: .seconds(leftAloneFor * 1.01 + 0.01))
            } catch {
                return
            }
        }
    }

    /// A place as the name of a task that asks around it: to a tenth of a
    /// degree, far finer than the edge the core asks again at, so that a
    /// pilot who moves is asked about anew and one who stands is not with
    /// every position.
    nonisolated static func askedKey(_ place: Coordinate?) -> String {
        guard let place else { return "" }
        return "\((place.lat * 10).rounded()),\((place.lon * 10).rounded())"
    }

    /// Removes what is kept from the server and stops asking for it.
    func stopUsingServer() {
        usesServer = false
        try? core.deleteKeptAirspace()
        serverProblem = nil
        reload()
    }

    private func reload() {
        files = (try? core.airspaceFiles()) ?? []
        kept = (try? core.keptAirspace()) ?? []
        problem = nil
        guard !isEmpty else {
            set = nil
            return
        }
        do {
            set = try core.airspace()
        } catch {
            set = nil
            problem = "Your airspace could not be loaded, so nothing is drawn or warned about. \(error.words)"
        }
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

    nonisolated static func count(_ airspaces: UInt32) -> String {
        airspaces == 1 ? "1 airspace" : "\(Format.count(airspaces)) airspaces"
    }
}
