import CryptoKit
import VentiCore
import XCTest

@testable import Venti

/// Maps for flying without a network, against the real core: the server
/// is played by the region that comes with the app, handed out in pieces.
@MainActor
final class OfflineMapsTests: XCTestCase {
    nonisolated private static let id = "tegernsee-demo"
    nonisolated private static let version = "2026-11-18"

    /// What was asked of the server for files, and how it should answer.
    private final class FileServer: @unchecked Sendable {
        private let lock = NSLock()
        private var asked: [(name: String, from: Int64)] = []
        private var breaksAfter: Int?
        private var goesOnFromTheMiddle = true
        let region: URL

        init(region: URL) {
            self.region = region
        }

        /// What was asked for, in order.
        var requests: [(name: String, from: Int64)] {
            lock.withLock { asked }
        }

        /// The next answer breaks off after this many bytes.
        func breakOff(after bytes: Int) {
            lock.withLock { breaksAfter = bytes }
        }

        /// Whether the server answers a request for part of a file with
        /// that part, or with the whole file.
        func goOnFromTheMiddle(_ does: Bool) {
            lock.withLock { goesOnFromTheMiddle = does }
        }

        func fetch(_ url: URL, from offset: Int64, take: @escaping @Sendable (Data) throws -> Void) async throws -> Int {
            let name = url.lastPathComponent
            let (limit, ranges): (Int?, Bool) = lock.withLock {
                asked.append((name, offset))
                let limit = breaksAfter
                breaksAfter = nil
                return (limit, goesOnFromTheMiddle)
            }

            guard url.path.hasPrefix("/v1/maps/\(OfflineMapsTests.id)/\(OfflineMapsTests.version)/"),
                  let file = try? Data(contentsOf: region.appendingPathComponent(name))
            else { return 404 }
            if offset > 0, !ranges {
                // The whole file where a part was asked for: none of it
                // is handed over.
                return 200
            }
            if offset >= Int64(file.count) {
                return 416
            }
            var sent = 0
            var at = Int(offset)
            while at < file.count {
                let end = min(at + 300_000, file.count)
                try take(file.subdata(in: at..<end))
                sent += end - at
                at = end
                if let limit, sent >= limit {
                    throw URLError(.networkConnectionLost)
                }
            }
            return offset > 0 ? 206 : 200
        }
    }

    private func sha256(of file: URL) throws -> String {
        SHA256.hash(data: try Data(contentsOf: file)).map { String(format: "%02x", $0) }.joined()
    }

    /// The server's list: the region that comes with the app, as a server
    /// would hand it out.
    private func list(terrainHash: String? = nil, version: String = OfflineMapsTests.version) throws -> Data {
        let region = try MapStyle.bundledRegion()
        let (basemap, terrain) = (
            region.appendingPathComponent("basemap.pmtiles"), region.appendingPathComponent("terrain.pmtiles")
        )
        let basemapSum = try sha256(of: basemap)
        let terrainSum = try terrainHash ?? sha256(of: terrain)
        let text = """
            {"regions": [{
                "id": "\(Self.id)", "name": "Tegernsee valley (demo)", "version": "\(version)",
                "covers": {"south": 47.60, "west": 11.64, "north": 47.76, "east": 11.86},
                "files": [
                    {"role": "basemap", "name": "basemap.pmtiles", "bytes": \(OfflineMapsModel.size(of: basemap)),
                     "sha256": "\(basemapSum)", "path": "",
                     "credits": [{"text": "© OpenStreetMap contributors",
                                  "url": "https://www.openstreetmap.org/copyright"}]},
                    {"role": "terrain", "name": "terrain.pmtiles", "bytes": \(OfflineMapsModel.size(of: terrain)),
                     "sha256": "\(terrainSum)", "path": "",
                     "credits": [{"text": "© Mapterhorn", "url": "https://mapterhorn.com/attribution"}]}
                ]}]}
            """
        return Data(text.utf8)
    }

    private func folder() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-maps-test-\(UUID().uuidString)", isDirectory: true)
    }

    private func model(in folder: URL, list: Data, server: FileServer) -> OfflineMapsModel {
        let model = OfflineMapsModel(
            folder: folder,
            transport: { url in
                url.path == "/v1/maps" ? (200, list) : (404, Data())
            },
            fetch: { url, offset, take in try await server.fetch(url, from: offset, take: take) },
            watchesNetwork: false
        )
        model.server = { URL(string: "https://venti.example.org") }
        return model
    }

    func testARegionIsDownloadedCheckedKeptAndDrawnFrom() async throws {
        let folder = folder()
        let server = FileServer(region: try MapStyle.bundledRegion())
        let maps = model(in: folder, list: try list(), server: server)
        var archives: [[String]] = []
        maps.onKeptChanged = { archives.append($0) }

        // Before the server has answered there is nothing to list.
        XCTAssertTrue(maps.rows.isEmpty)
        XCTAssertEqual(maps.inALine, "None")
        await maps.refresh()
        let row = try XCTUnwrap(maps.rows.first)
        XCTAssertEqual(row.state, .offered)
        XCTAssertEqual(row.words, "7.5 MB")

        maps.download(row.region)
        XCTAssertNotNil(maps.progress[Self.id])
        await maps.wait(for: Self.id)

        // Both files are in the region's place, with the record of what
        // they are, and nothing is left where downloads are put together.
        XCTAssertNil(maps.progress[Self.id])
        XCTAssertEqual(maps.kept.map(\.id), [Self.id])
        XCTAssertEqual(maps.inALine, "Tegernsee valley (demo)")
        XCTAssertEqual(maps.rows.first?.state, .kept)
        XCTAssertEqual(maps.rows.first?.words, "On this device, 7.5 MB")
        let place = folder.appendingPathComponent(Self.id).appendingPathComponent(Self.version)
        for name in ["basemap.pmtiles", "terrain.pmtiles", OfflineMapsModel.recordName] {
            XCTAssertTrue(FileManager.default.fileExists(atPath: place.appendingPathComponent(name).path), name)
        }
        let gathering = OfflineMapsModel.incoming(in: folder).appendingPathComponent(Self.id)
        XCTAssertEqual((try? FileManager.default.contentsOfDirectory(atPath: gathering.path)) ?? [], [])
        XCTAssertEqual(server.requests.map { $0.name }, ["basemap.pmtiles", "terrain.pmtiles"])
        XCTAssertEqual(server.requests.map { $0.from }, [0, 0])
        // The ground is told where the region's elevation is.
        XCTAssertEqual(archives.last, [place.appendingPathComponent("terrain.pmtiles").path])

        // The cockpit's map is drawn from the region where the glider is
        // inside it, and as before where it is not.
        XCTAssertEqual(maps.flyingSource, .standard)
        maps.flying(at: Coordinate(lat: 47.6642, lon: 11.7967))
        guard case .kept(let drawn) = maps.flyingSource else {
            return XCTFail("The cockpit does not draw from the region: \(maps.flyingSource)")
        }
        XCTAssertEqual(drawn.basemap.path, place.appendingPathComponent("basemap.pmtiles").path)
        XCTAssertEqual(maps.flyingSource.key, "kept-tegernsee-demo-2026-11-18")
        let style = try MapStyle.json(for: maps.flyingSource)
        XCTAssertTrue(style.contains("pmtiles://file://"))
        XCTAssertTrue(style.contains("/tegernsee-demo/2026-11-18/terrain.pmtiles"))
        maps.flying(at: Coordinate(lat: 46.0, lon: 8.0))
        XCTAssertEqual(maps.flyingSource, .standard)

        // The other maps come from the network while there is one, and
        // from the region when there is none.
        let inside = Coordinate(lat: 47.70, lon: 11.75)
        XCTAssertEqual(maps.browsingSource(at: inside), .standard)
        maps.setOnline(false)
        XCTAssertEqual(maps.browsingSource(at: inside).key, "kept-tegernsee-demo-2026-11-18")
        XCTAssertEqual(maps.browsingSource(at: nil).key, "kept-tegernsee-demo-2026-11-18")
        XCTAssertEqual(maps.browsingSource(at: Coordinate(lat: 46.0, lon: 8.0)), .standard)
        maps.setOnline(true)

        // The app started again finds the region without asking anybody.
        let again = OfflineMapsModel(
            folder: folder, transport: { _ in (500, Data()) }, fetch: { _, _, _ in 500 }, watchesNetwork: false
        )
        XCTAssertEqual(again.kept.map(\.version), [Self.version])
        XCTAssertEqual(again.rows.first?.state, .kept)

        // A newer edition on the server is offered beside what is kept.
        let newer = model(in: folder, list: try list(version: "2026-12-01"), server: server)
        await newer.refresh()
        XCTAssertEqual(newer.rows.first?.state, .keptWithNewer)
        XCTAssertEqual(newer.rows.first?.newer?.version, "2026-12-01")

        // Deleted, it is gone from the iPhone and from the maps.
        maps.flying(at: Coordinate(lat: 47.6642, lon: 11.7967))
        maps.delete(try XCTUnwrap(maps.kept.first))
        XCTAssertTrue(maps.kept.isEmpty)
        XCTAssertEqual(maps.flyingSource, .standard)
        XCTAssertEqual(archives.last, [])
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.appendingPathComponent(Self.id).path))
        XCTAssertEqual(maps.rows.first?.state, .offered)
    }

    func testADownloadThatBreaksOffGoesOnFromWhereItStopped() async throws {
        let folder = folder()
        let server = FileServer(region: try MapStyle.bundledRegion())
        let maps = model(in: folder, list: try list(), server: server)
        await maps.refresh()
        let region = try XCTUnwrap(maps.rows.first?.region)

        // The connection is lost 600 kB into the base map.
        server.breakOff(after: 600_000)
        maps.download(region)
        await maps.wait(for: Self.id)

        guard case .stopped(let words)? = maps.progress[Self.id] else {
            return XCTFail("The download did not stop: \(String(describing: maps.progress[Self.id]))")
        }
        XCTAssertTrue(words.contains("goes on from there"), words)
        XCTAssertTrue(maps.kept.isEmpty)
        let part = OfflineMapsModel.incoming(in: folder)
            .appendingPathComponent(Self.id).appendingPathComponent(Self.version)
            .appendingPathComponent("basemap.pmtiles")
        XCTAssertEqual(OfflineMapsModel.size(of: part), 600_000)

        // The next attempt asks for the rest only, and the map is whole.
        maps.download(region)
        await maps.wait(for: Self.id)

        XCTAssertNil(maps.progress[Self.id])
        XCTAssertEqual(maps.kept.map(\.id), [Self.id])
        XCTAssertEqual(server.requests.map { $0.name }, ["basemap.pmtiles", "basemap.pmtiles", "terrain.pmtiles"])
        XCTAssertEqual(server.requests.map { $0.from }, [0, 600_000, 0])
    }

    func testAServerThatDoesNotGoOnFromTheMiddleIsAskedFromTheStart() async throws {
        let folder = folder()
        let server = FileServer(region: try MapStyle.bundledRegion())
        let maps = model(in: folder, list: try list(), server: server)
        await maps.refresh()
        let region = try XCTUnwrap(maps.rows.first?.region)

        server.breakOff(after: 300_000)
        maps.download(region)
        await maps.wait(for: Self.id)
        server.goOnFromTheMiddle(false)
        maps.download(region)
        await maps.wait(for: Self.id)

        // Asked for the rest, answered with the whole: taken from the
        // start, and whole and right in the end.
        XCTAssertEqual(maps.kept.map(\.id), [Self.id])
        XCTAssertEqual(server.requests.map { $0.from }, [0, 300_000, 0, 0])
    }

    func testADownloadThatIsNotWhatTheServerDescribedIsThrownAway() async throws {
        let folder = folder()
        let server = FileServer(region: try MapStyle.bundledRegion())
        let wrong = String(repeating: "0", count: 64)
        let maps = model(in: folder, list: try list(terrainHash: wrong), server: server)
        await maps.refresh()
        let region = try XCTUnwrap(maps.rows.first?.region)

        maps.download(region)
        await maps.wait(for: Self.id)

        guard case .stopped(let words)? = maps.progress[Self.id] else {
            return XCTFail("A file that is not the server's was kept")
        }
        XCTAssertTrue(words.contains("not what the server described"), words)
        XCTAssertTrue(maps.kept.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.appendingPathComponent(Self.id).path))
        // The file that failed is gone; the one that was right stays for
        // the next attempt.
        let gathering = OfflineMapsModel.incoming(in: folder)
            .appendingPathComponent(Self.id).appendingPathComponent(Self.version)
        XCTAssertFalse(FileManager.default.fileExists(atPath: gathering.appendingPathComponent("terrain.pmtiles").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: gathering.appendingPathComponent("basemap.pmtiles").path))

        // Without a server there is nothing to download from, and a
        // model that keeps nothing says so instead of fetching.
        let nowhere = OfflineMapsModel(
            folder: nil, transport: { _ in (500, Data()) }, fetch: { _, _, _ in 500 }, watchesNetwork: false
        )
        nowhere.download(region)
        XCTAssertEqual(nowhere.progress[Self.id], .stopped("There is no server to download from."))
    }
}
