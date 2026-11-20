import Foundation
import VentiCore

/// The ground: the map's elevation tiles around the place the pilot flies
/// at, for the cockpit's height above ground, for airspace given above
/// ground and for the side view.
///
/// Tiles are the ones the map draws its relief from. Each is fetched once
/// and kept on the iPhone, so that the ground is there again without a
/// network: over the place flown at last time, and wherever a flight led
/// while there was one. The core reads the heights out of them; nothing
/// here knows what a tile holds.
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
    @ObservationIgnored private var hasReadArchive = false

    /// How many moments of a flight the ground is read at for its
    /// profile: more than the chart is points wide.
    static let placesUnderAFlight: UInt32 = 240

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
        switch MapStyle.Source.current {
        case .online:
            return Source.server(MapStyle.Online.terrainTiles)
        case .bundled:
            guard let region = try? MapStyle.bundledRegion() else {
                return Source.nowhere
            }
            return Source.archive(region.appendingPathComponent("terrain.pmtiles", isDirectory: false))
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
        switch source {
        case .nowhere:
            return []
        case .archive:
            // The region that comes with the app is small: it is read
            // once, into the terrain the cockpit uses too.
            await readArchive()
            let terrain = terrain
            return await Task.detached(priority: .utility) {
                terrain.groundUnder(track: track, places: Self.placesUnderAFlight)
            }.value
        case .server(let template):
            // A terrain of its own, which goes when the flight is closed:
            // the cockpit's holds the ground around the glider and no more.
            let under = Terrain()
            let (folder, transport) = (folder, transport)
            for tile in under.tilesCovering(area: flight.bounds) {
                await Self.bring(tile, into: under, from: template, keptIn: folder, by: transport)
            }
            return await Task.detached(priority: .utility) {
                under.groundUnder(track: track, places: Self.placesUnderAFlight)
            }.value
        }
    }

    /// Reads the archive inside the app into the terrain, once.
    private func readArchive() async {
        guard case .archive(let file) = source, !hasReadArchive else { return }
        hasReadArchive = true
        let terrain = terrain
        // Reading and decoding take a moment: not on the main thread.
        await Task.detached(priority: .utility) {
            guard let archive = try? Data(contentsOf: file) else { return }
            _ = try? terrain.addArchive(archive: archive)
        }.value
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
