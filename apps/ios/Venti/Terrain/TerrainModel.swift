import Foundation
import VentiCore

/// The ground: the map's elevation tiles around the place the pilot flies
/// at, for the cockpit's height above ground, for airspace given above
/// ground and for the side view.
///
/// Tiles are the ones the map draws its relief from. Where the pilot has
/// downloaded a region's map, they are read from its elevation archive on
/// the iPhone. Elsewhere each is fetched once and kept, so that the ground
/// is there again without a network: over the place flown at last time,
/// and wherever a flight led while there was one. The core reads the
/// heights out of them; nothing here knows what a tile holds.
@MainActor
@Observable
final class TerrainModel {
    /// Fetches one tile: the status and the bytes.
    typealias Transport = @Sendable (URL) async throws -> (Int, Data)

    /// Where the tiles come from.
    enum Source: Sendable {
        /// A tile server: its address with `{z}/{x}/{y}` for the tile.
        case server(String)
        /// An archive inside the app, read once. Needs no network.
        case archive(URL)
        /// Nowhere: the cockpit then knows no ground.
        case nowhere
    }

    /// The heights, as the flying session uses them.
    @ObservationIgnored let terrain = Terrain()

    @ObservationIgnored private let source: Source
    @ObservationIgnored private let transport: Transport
    /// The folder the tiles are kept in; `nil` keeps none.
    @ObservationIgnored private let folder: URL?
    @ObservationIgnored private var isLoading = false
    /// The reading of the archive inside the app, once begun: whoever
    /// wants the ground waits for it to end.
    @ObservationIgnored private var archiveRead: Task<Void, Never>?
    /// The elevation archives of the regions kept on the iPhone, by
    /// their files: opened in the terrain, and read before anything is
    /// fetched.
    @ObservationIgnored private var archives: [String] = []

    /// How many moments of a flight the ground is read at for its
    /// profile: more than the chart is points wide.
    nonisolated static let placesUnderAFlight: UInt32 = 240

    init(
        source: Source = TerrainModel.sourceInUse,
        folder: URL? = TerrainModel.store,
        transport: @escaping Transport = TerrainModel.fetch
    ) {
        self.source = source
        self.folder = folder
        self.transport = transport
    }

    /// The source the app draws its map from: the public tile server, or
    /// the region inside the app when the map is drawn from that.
    static var sourceInUse: Source {
        switch MapStyle.Source.standard {
        case .online:
            return Source.server(MapStyle.Online.terrainTiles)
        case .bundled:
            guard let region = try? MapStyle.bundledRegion() else {
                return Source.nowhere
            }
            return Source.archive(region.appendingPathComponent("terrain.pmtiles", isDirectory: false))
        case .kept:
            // Kept regions come on top of the source, with `use(archives:)`.
            return Source.nowhere
        }
    }

    /// Takes the elevation archives of the regions kept on the iPhone, in
    /// place of those taken before: the ground is read from them wherever
    /// they reach. An archive that cannot be opened is left out; its
    /// region's ground then comes from the network as before.
    func use(archives paths: [String]) {
        archives = paths
        terrain.closeArchives()
        for path in paths {
            try? terrain.openArchive(path: path)
        }
    }

    /// Where tiles are kept on the iPhone: beside the app's other data and
    /// not in its backup, since they can be fetched again. Under test,
    /// nowhere.
    static var store: URL? {
        guard !LaunchOptions.isUnderTest,
              var folder = try? FileManager.default.url(
                  for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
              ).appendingPathComponent("Terrain", isDirectory: true)
        else { return nil }
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? folder.setResourceValues(values)
        return folder
    }

    /// Brings the tiles around a place that are not there yet: from the
    /// iPhone's store where it has them, else from the tile server. Tiles
    /// far from the place are let go from memory; they stay in the store.
    /// Does nothing while an earlier call is still at work.
    func load(around place: Coordinate) async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }

        // What a kept region has is read from its file first; only what
        // it lacks is fetched.
        if !archives.isEmpty {
            let terrain = terrain
            _ = await Task.detached(priority: .utility) {
                terrain.readAround(lat: place.lat, lon: place.lon)
            }.value
        }

        switch source {
        case .nowhere:
            return
        case .archive:
            await readArchive()
        case .server(let template):
            let (terrain, folder, transport) = (terrain, folder, transport)
            for tile in terrain.tilesWanted(lat: place.lat, lon: place.lon) {
                await Self.bring(tile, into: terrain, from: template, keptIn: folder, by: transport)
            }
            terrain.keepAround(lat: place.lat, lon: place.lon)
        }
    }

    /// The ground under a flight's track, for its profile: the tiles
    /// under the flight are brought, from the iPhone's store or the tile
    /// server, read, and let go again. Empty where there is no ground to
    /// be had.
    func ground(under flight: FlightView) async -> [GroundUnderFlight] {
        let track = flight.track
        let under = await ground(covering: flight.bounds)
        return await Task.detached(priority: .utility) {
            under.groundUnder(track: track, places: Self.placesUnderAFlight)
        }.value
    }

    /// The ground under an area: a flight's, or a planned route's for the
    /// heights to leave its places at. What kept regions have of it is
    /// read, then the tiles it lacks are brought from the iPhone's store
    /// or the tile server. How many tiles, at which zoom, is the core's
    /// (`tilesCovering`). Where there is none to be had, a terrain that
    /// knows no ground.
    func ground(covering area: Bounds) async -> Terrain {
        // A terrain of its own, which goes when its user lets it go: the
        // cockpit's holds the ground around the glider and no more.
        let under: Terrain
        switch source {
        case .archive:
            // The region that comes with the app is small: it is read
            // once, into the terrain the cockpit uses too.
            await readArchive()
            under = terrain
        case .server, .nowhere:
            under = Terrain()
            for path in archives {
                try? under.openArchive(path: path)
            }
        }
        // First what the kept regions have, from their files.
        if !archives.isEmpty {
            _ = await Task.detached(priority: .utility) {
                under.readCovering(area: area)
            }.value
        }
        if case .server(let template) = source {
            let (folder, transport) = (folder, transport)
            for tile in under.tilesCovering(area: area) {
                await Self.bring(tile, into: under, from: template, keptIn: folder, by: transport)
            }
        }
        return under
    }

    /// Reads the archive inside the app into the terrain, once.
    private func readArchive() async {
        guard case .archive(let file) = source else { return }
        if archiveRead == nil {
            let terrain = terrain
            // Reading and decoding take a moment: not on the main thread.
            archiveRead = Task.detached(priority: .utility) {
                guard let archive = try? Data(contentsOf: file) else { return }
                _ = try? terrain.addArchive(archive: archive)
            }
        }
        await archiveRead?.value
    }

    /// Brings one tile into a terrain: from the iPhone's store if it is
    /// there, else from the tile server, and then into the store. Off the
    /// main thread: a tile is read and decoded in a few hundredths of a
    /// second, and there are dozens.
    nonisolated private static func bring(
        _ tile: TerrainTile, into terrain: Terrain, from template: String, keptIn folder: URL?,
        by transport: @escaping Transport
    ) async {
        await Task.detached(priority: .utility) {
            let kept = folder?.appendingPathComponent("\(tile.z)-\(tile.x)-\(tile.y).webp", isDirectory: false)
            if let kept, let image = try? Data(contentsOf: kept),
               (try? terrain.addTile(z: tile.z, x: tile.x, y: tile.y, image: image)) != nil
            {
                return
            }
            guard let address = URL(string: template.replacingOccurrences(of: "{z}/{x}/{y}", with: tile.path)),
                  let (status, image) = try? await transport(address), status == 200,
                  (try? terrain.addTile(z: tile.z, x: tile.x, y: tile.y, image: image)) != nil
            else { return }
            if let kept {
                try? image.write(to: kept, options: .atomic)
            }
        }.value
    }

    /// Fetches with the system's networking and its cache: the map asks
    /// for the same tiles.
    nonisolated static func fetch(_ address: URL) async throws -> (Int, Data) {
        var request = URLRequest(url: address)
        request.timeoutInterval = 20
        let (data, response) = try await URLSession.shared.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }
}
