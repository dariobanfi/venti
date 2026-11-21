import Foundation
import Network
import Observation
import VentiCore

/// Maps for flying without a network: the regions the pilot's Venti
/// server hands out, and the ones kept on the iPhone.
///
/// A region is two files, the base map and the elevation. The core reads
/// the server's list, checks a download against it and says which kept
/// region covers a place; this class fetches the files, keeps them, and
/// tells each map what to draw from.
///
/// A download runs while the app is open. When it breaks off, what has
/// arrived stays, and the download goes on from there the next time.
@MainActor
@Observable
final class OfflineMapsModel {
    /// Fetches an address and returns the HTTP status and the body.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// Fetches a file from one of its bytes on and hands it over in
    /// pieces as they arrive. Returns the HTTP status. Pieces are handed
    /// over only of an answer that is the file from that byte on: status
    /// 200 for a fetch from the start, 206 for one from further in.
    typealias Fetch = @Sendable (URL, Int64, @escaping @Sendable (Data) throws -> Void) async throws -> Int

    /// Where a download stands.
    enum Progress: Equatable {
        case downloading(received: Int64, total: Int64)
        /// Everything has arrived and is being checked against what the
        /// server said.
        case checking
        /// It stopped, and the words say why. What arrived is kept.
        case stopped(String)
    }

    /// What a download reports while it runs.
    enum Step: Sendable {
        case received(Int64)
        case checking
    }

    /// How a download ended.
    enum Outcome: Sendable, Equatable {
        case kept
        case cancelled
        case failed(String)
    }

    /// A download that cannot go on, with the reason for the pilot.
    struct Stopped: Error {
        let words: String
    }

    /// The name of the file in a region's folder that says what the
    /// region is.
    nonisolated static let recordName = "region.json"

    /// The regions kept on the iPhone, by name.
    private(set) var kept: [MapRegionView] = []
    /// What the server hands out. `nil` until it has answered.
    private(set) var offered: [MapRegionView]?
    private(set) var isLoading = false
    /// Why the server's list is not there, in words for the pilot.
    private(set) var problem: String?
    /// The downloads under way or stopped, by region.
    private(set) var progress: [String: Progress] = [:]
    /// Whether the iPhone has a network, as far as it knows.
    private(set) var isOnline = true
    /// What the cockpit's map is drawn from: the kept region that covers
    /// the glider, where there is one.
    private(set) var flyingSource: MapStyle.Source = .standard

    /// The server to ask. The app sets this to the one in the settings.
    @ObservationIgnored var server: () -> URL? = { nil }
    /// Called with the elevation archives of the kept regions whenever
    /// the regions change.
    @ObservationIgnored var onKeptChanged: ([String]) -> Void = { _ in }

    /// Where regions are kept; `nil` keeps none.
    @ObservationIgnored private let folder: URL?
    @ObservationIgnored private let transport: Transport
    @ObservationIgnored private let fetchFile: Fetch
    @ObservationIgnored private var tasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored private let monitor: NWPathMonitor?

    init(
        folder: URL? = OfflineMapsModel.store,
        transport: @escaping Transport = WindModel.fetch,
        fetch: @escaping Fetch = OfflineMapsModel.fetch,
        watchesNetwork: Bool = !LaunchOptions.isUnderTest
    ) {
        self.folder = folder
        self.transport = transport
        fetchFile = fetch
        monitor = watchesNetwork ? NWPathMonitor() : nil
        kept = Self.regions(in: folder)
        monitor?.pathUpdateHandler = { path in
            let isOnline = path.status == .satisfied
            Task { @MainActor in
                self.setOnline(isOnline)
            }
        }
        monitor?.start(queue: DispatchQueue(label: "venti.network"))
    }

    /// Where regions are kept on the iPhone: beside the app's other data
    /// and not in its backup, since they can be downloaded again. Under
    /// test, nowhere.
    static var store: URL? {
        guard !LaunchOptions.isUnderTest,
              var folder = try? FileManager.default.url(
                  for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
              ).appendingPathComponent("Maps", isDirectory: true)
        else { return nil }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? folder.setResourceValues(values)
        return folder
    }

    // MARK: What there is

    /// The list for the pilot: what the iPhone has first, then what else
    /// the server hands out.
    var rows: [MapRegionRow] {
        mapRows(offered: offered ?? [], kept: kept)
    }

    /// The kept regions in a line: "None", a region's name, or how many.
    var inALine: String {
        switch kept.count {
        case 0: "None"
        case 1: kept[0].name
        default: "\(kept.count) regions"
        }
    }

    /// The elevation archives of the kept regions, by their files.
    var terrainArchives: [String] {
        guard let folder else { return [] }
        return kept.map { region in
            Self.place(of: region, in: folder).appendingPathComponent(region.terrain.name, isDirectory: false).path
        }
    }

    /// Asks the server which regions it hands out.
    func refresh() async {
        guard let server = server() else {
            offered = nil
            problem = nil
            return
        }
        isLoading = true
        defer { isLoading = false }
        do {
            guard let url = URL(string: server.absoluteString + mapsPath()) else { return }
            let (status, body) = try await transport(url)
            let regions = try mapRegions(status: UInt16(clamping: status), body: body)
            // The pilot may have changed the server while this was under way.
            guard server == self.server() else { return }
            offered = regions
            problem = nil
        } catch is CancellationError {
            return
        } catch {
            if (error as? URLError)?.code == .cancelled {
                return
            }
            problem = error is CoreError
                ? error.words
                : "The server could not be reached for its maps. \(error.localizedDescription)"
        }
    }

    // MARK: What a map is drawn from

    /// The cockpit says where the glider is: its map is drawn from the
    /// kept region that covers the place, with a network or without.
    func flying(at place: Coordinate) {
        let region = mapRegionCovering(kept: kept, lat: place.lat, lon: place.lon)
        let drawnFrom: MapStyle.Source = region.flatMap { self.source(of: $0) } ?? MapStyle.Source.standard
        if drawnFrom != flyingSource {
            flyingSource = drawnFrom
        }
    }

    /// What a map for looking around is drawn from: the network while
    /// there is one, since it reaches everywhere; without one, the kept
    /// region that covers the place, or with no place named, the first
    /// kept region.
    func browsingSource(at place: Coordinate?) -> MapStyle.Source {
        guard !isOnline, !kept.isEmpty else { return .standard }
        let region: MapRegionView? =
            if let place {
                mapRegionCovering(kept: kept, lat: place.lat, lon: place.lon)
            } else {
                kept.first
            }
        return region.flatMap { self.source(of: $0) } ?? MapStyle.Source.standard
    }

    /// A kept region as a source for the map's style.
    func source(of region: MapRegionView) -> MapStyle.Source? {
        guard let folder else { return nil }
        let place = Self.place(of: region, in: folder)
        return .kept(
            MapStyle.Kept(
                id: region.id,
                version: region.version,
                basemap: place.appendingPathComponent(region.basemap.name, isDirectory: false),
                terrain: place.appendingPathComponent(region.terrain.name, isDirectory: false)
            )
        )
    }

    /// The network came or went.
    func setOnline(_ isOnline: Bool) {
        if isOnline != self.isOnline {
            self.isOnline = isOnline
        }
    }

    // MARK: Downloading

    /// Downloads a region, or goes on with a download that stopped.
    func download(_ region: MapRegionView) {
        guard tasks[region.id] == nil else { return }
        guard let server = server(), let folder else {
            progress[region.id] = .stopped("There is no server to download from.")
            return
        }
        let id = region.id
        progress[id] = .downloading(received: 0, total: Int64(clamping: region.bytes))
        let fetch = fetchFile
        tasks[id] = Task {
            let outcome = await Self.bring(region, from: server, into: folder, by: fetch) { step in
                Task { @MainActor in
                    self.note(step, for: id)
                }
            }
            self.finished(region, with: outcome)
        }
    }

    /// Stops a download. What has arrived stays for the next time.
    func stop(_ id: String) {
        tasks[id]?.cancel()
    }

    /// Waits until the download of a region has ended. For tests.
    func wait(for id: String) async {
        _ = await tasks[id]?.value
    }

    /// Removes a region from the iPhone, with anything of it that was
    /// under way.
    func delete(_ region: MapRegionView) {
        guard let folder else { return }
        tasks[region.id]?.cancel()
        let files = FileManager.default
        try? files.removeItem(at: folder.appendingPathComponent(region.id, isDirectory: true))
        try? files.removeItem(at: Self.incoming(in: folder).appendingPathComponent(region.id, isDirectory: true))
        progress[region.id] = nil
        keptChanged()
    }

    private func note(_ step: Step, for id: String) {
        // A step that arrives after the download has ended says nothing new.
        guard tasks[id] != nil else { return }
        switch step {
        case .received(let received):
            if case .downloading(_, let total)? = progress[id] {
                progress[id] = .downloading(received: received, total: total)
            }
        case .checking:
            progress[id] = .checking
        }
    }

    private func finished(_ region: MapRegionView, with outcome: Outcome) {
        tasks[region.id] = nil
        switch outcome {
        case .kept:
            progress[region.id] = nil
            keptChanged()
        case .cancelled:
            progress[region.id] = nil
        case .failed(let words):
            progress[region.id] = .stopped(words)
        }
    }

    private func keptChanged() {
        kept = Self.regions(in: folder)
        // A map is not left drawing from files that are gone.
        if case .kept(let drawn) = flyingSource,
           !kept.contains(where: { $0.id == drawn.id && $0.version == drawn.version })
        {
            flyingSource = .standard
        }
        onKeptChanged(terrainArchives)
    }

    // MARK: Files

    /// The folder of a kept region: its identifier, then its version.
    nonisolated static func place(of region: MapRegionView, in folder: URL) -> URL {
        folder
            .appendingPathComponent(region.id, isDirectory: true)
            .appendingPathComponent(region.version, isDirectory: true)
    }

    /// Where downloads are put together before they are whole. Its name
    /// begins with a dot, which no region's identifier does.
    nonisolated static func incoming(in folder: URL) -> URL {
        folder.appendingPathComponent(".incoming", isDirectory: true)
    }

    /// The regions whose files are whole in a folder, by name.
    nonisolated static func regions(in folder: URL?) -> [MapRegionView] {
        let files = FileManager.default
        guard let folder,
              let ids = try? files.contentsOfDirectory(
                  at: folder, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
              )
        else { return [] }
        var found: [MapRegionView] = []
        for id in ids {
            let versions =
                (try? files.contentsOfDirectory(at: id, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]))
                ?? []
            for version in versions {
                let record = version.appendingPathComponent(recordName, isDirectory: false)
                guard let data = try? Data(contentsOf: record),
                      let region = try? mapRegionFromRecord(record: data),
                      region.id == id.lastPathComponent, region.version == version.lastPathComponent,
                      size(of: version.appendingPathComponent(region.basemap.name)) == Int64(clamping: region.basemap.bytes),
                      size(of: version.appendingPathComponent(region.terrain.name)) == Int64(clamping: region.terrain.bytes)
                else { continue }
                found.append(region)
                // One version of a region is kept.
                break
            }
        }
        return found.sorted { $0.name < $1.name }
    }

    /// The size of a file; zero for one that is not there.
    nonisolated static func size(of file: URL) -> Int64 {
        let attributes = try? FileManager.default.attributesOfItem(atPath: file.path)
        return (attributes?[.size] as? NSNumber)?.int64Value ?? 0
    }

    /// Brings a region to the iPhone: fetches its two files, going on
    /// from what an earlier attempt left, checks them against what the
    /// server said, and puts them in the region's place.
    nonisolated static func bring(
        _ region: MapRegionView, from server: URL, into folder: URL, by fetch: @escaping Fetch,
        tell: @escaping @Sendable (Step) -> Void
    ) async -> Outcome {
        let files = FileManager.default
        let gathering = incoming(in: folder)
            .appendingPathComponent(region.id, isDirectory: true)
            .appendingPathComponent(region.version, isDirectory: true)
        let parts = [region.basemap, region.terrain]
        do {
            try files.createDirectory(at: gathering, withIntermediateDirectories: true)

            // Room for what is still to come, and a little more.
            let here = parts.reduce(Int64(0)) { sum, file in
                sum + size(of: gathering.appendingPathComponent(file.name, isDirectory: false))
            }
            let room = try? folder.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
                .volumeAvailableCapacityForImportantUsage
            if let room, room < Int64(clamping: region.bytes) - here + 50_000_000 {
                return .failed("There is not enough room on this iPhone: the map needs \(region.size).")
            }

            var before: Int64 = 0
            for file in parts {
                let part = gathering.appendingPathComponent(file.name, isDirectory: false)
                let done = before
                try await bring(file, from: server, to: part, by: fetch) { received in
                    tell(.received(done + received))
                }
                before += Int64(clamping: file.bytes)
            }
            try Task.checkCancellation()

            tell(.checking)
            for file in parts {
                let part = gathering.appendingPathComponent(file.name, isDirectory: false)
                do {
                    try checkMapFile(path: part.path, file: file)
                } catch {
                    // A file that is not the server's is of no use, and
                    // going on from it would not mend it.
                    try? files.removeItem(at: part)
                    return .failed(error.words)
                }
            }
            try mapRegionRecord(region: region)
                .write(to: gathering.appendingPathComponent(recordName, isDirectory: false), options: .atomic)

            // Into the region's place; an older version goes once the new
            // one is there.
            let home = folder.appendingPathComponent(region.id, isDirectory: true)
            try files.createDirectory(at: home, withIntermediateDirectories: true)
            let place = home.appendingPathComponent(region.version, isDirectory: true)
            try? files.removeItem(at: place)
            try files.moveItem(at: gathering, to: place)
            let versions = (try? files.contentsOfDirectory(at: home, includingPropertiesForKeys: nil)) ?? []
            for other in versions where other.lastPathComponent != region.version {
                try? files.removeItem(at: other)
            }
            return .kept
        } catch is CancellationError {
            return .cancelled
        } catch let stopped as Stopped {
            return .failed(stopped.words)
        } catch {
            if Task.isCancelled || (error as? URLError)?.code == .cancelled {
                return .cancelled
            }
            return .failed(
                "The download stopped: \(error.localizedDescription) What has arrived is kept, and the download goes on from there."
            )
        }
    }

    /// Brings one file into its place among a download's parts, going on
    /// from the bytes that are there already.
    nonisolated private static func bring(
        _ file: MapFileView, from server: URL, to part: URL, by fetch: Fetch,
        tell: @escaping @Sendable (Int64) -> Void
    ) async throws {
        let files = FileManager.default
        let total = Int64(clamping: file.bytes)
        guard let url = URL(string: server.absoluteString + file.path) else {
            throw Stopped(words: "Venti could not be asked for this download.")
        }
        // Twice at the most: once more from the start when the server does
        // not go on from where the file stopped.
        for _ in 0..<2 {
            var have = size(of: part)
            if have > total {
                try? files.removeItem(at: part)
                have = 0
            }
            if have == total {
                tell(total)
                return
            }
            if !files.fileExists(atPath: part.path) {
                files.createFile(atPath: part.path, contents: nil)
            }
            let writer = try PartWriter(part, holding: have, tell: tell)
            let status: Int
            do {
                status = try await fetch(url, have, writer.take)
                writer.close()
            } catch {
                writer.close()
                throw error
            }
            switch status {
            case 206 where have > 0, 200 where have == 0:
                if size(of: part) == total {
                    tell(total)
                    return
                }
                throw Stopped(
                    words: "The download stopped before the map was whole. What has arrived is kept, and the download goes on from there."
                )
            case 200, 206, 416:
                // Not the file from where it stopped: from the start, then.
                try? files.removeItem(at: part)
            case 404:
                throw Stopped(words: "The server no longer has this map. Pull down to see what it has now.")
            default:
                throw Stopped(words: "The server refused the download (status \(status)).")
            }
        }
        throw Stopped(words: "The server does not hand this map out in a way that can be downloaded.")
    }

    /// Fetches with the system's networking, piece by piece, so that a
    /// file of hundreds of megabytes never lies in memory.
    nonisolated static func fetch(
        _ url: URL, from offset: Int64, take: @escaping @Sendable (Data) throws -> Void
    ) async throws -> Int {
        var request = URLRequest(url: url)
        // A minute without a byte ends the attempt; the whole may take hours.
        request.timeoutInterval = 60
        request.cachePolicy = .reloadIgnoringLocalCacheData
        // The bytes as they are: a packed answer could not be gone on with.
        request.setValue("identity", forHTTPHeaderField: "Accept-Encoding")
        if offset > 0 {
            request.setValue("bytes=\(offset)-", forHTTPHeaderField: "Range")
        }
        let receiver = PieceReceiver(wanted: offset > 0 ? 206 : 200, take: take)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.urlCache = nil
        let session = URLSession(configuration: configuration, delegate: receiver, delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        let task = session.dataTask(with: request)
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                receiver.begin(continuation)
                task.resume()
            }
        } onCancel: {
            task.cancel()
        }
    }
}

/// Appends what arrives to a file and says how much of it is there. Its
/// pieces come one after the other, from one queue.
private final class PartWriter: @unchecked Sendable {
    private let handle: FileHandle
    private var written: Int64
    private var told = Date.distantPast
    private let tell: @Sendable (Int64) -> Void

    init(_ file: URL, holding written: Int64, tell: @escaping @Sendable (Int64) -> Void) throws {
        let handle = try FileHandle(forWritingTo: file)
        _ = try handle.seekToEnd()
        self.handle = handle
        self.written = written
        self.tell = tell
    }

    var take: @Sendable (Data) throws -> Void {
        { data in
            try self.handle.write(contentsOf: data)
            self.written += Int64(data.count)
            // A few times a second is enough for a bar that fills.
            if Date().timeIntervalSince(self.told) > 0.25 {
                self.told = Date()
                self.tell(self.written)
            }
        }
    }

    func close() {
        try? handle.close()
    }
}

/// Receives one answer piece by piece. The session calls it on a queue of
/// its own, one call at a time.
private final class PieceReceiver: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    /// The status of an answer that is the file asked for.
    private let wanted: Int
    private let take: @Sendable (Data) throws -> Void
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Int, any Error>?
    private var failure: (any Error)?

    init(wanted: Int, take: @escaping @Sendable (Data) throws -> Void) {
        self.wanted = wanted
        self.take = take
    }

    func begin(_ continuation: CheckedContinuation<Int, any Error>) {
        lock.withLock { self.continuation = continuation }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        // The body of a refusal, or of a whole file where a part was asked
        // for, is not the file from where it stopped: it is let pass.
        guard (dataTask.response as? HTTPURLResponse)?.statusCode == wanted, failure == nil else { return }
        do {
            try take(data)
        } catch {
            failure = error
            dataTask.cancel()
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: (any Error)?) {
        let continuation = lock.withLock {
            let waiting = self.continuation
            self.continuation = nil
            return waiting
        }
        if let failure {
            continuation?.resume(throwing: failure)
        } else if let error {
            continuation?.resume(throwing: error)
        } else {
            continuation?.resume(returning: (task.response as? HTTPURLResponse)?.statusCode ?? 0)
        }
    }
}
