import VentiCore
import XCTest

@testable import Venti

/// The ground: the elevation tiles that come with the app, read by the
/// real core, and the tile server played by a closure.
@MainActor
final class TerrainTests: XCTestCase {
    /// What was asked of the tile server.
    private actor Asked {
        var addresses: [URL] = []
        func note(_ address: URL) { addresses.append(address) }
    }

    func testTheRegionInsideTheAppGivesTheHeightOfTheGround() async throws {
        let region = try MapStyle.bundledRegion()
        let model = TerrainModel(
            source: .archive(region.appendingPathComponent("terrain.pmtiles", isDirectory: false)),
            folder: nil,
            transport: { _ in
                XCTFail("Nothing is fetched when the ground comes with the app")
                return (500, Data())
            }
        )
        XCTAssertNil(model.terrain.elevation(lat: 47.713, lon: 11.74))

        await model.load(around: Coordinate(lat: 47.6642, lon: 11.7967))

        // The Tegernsee lies at 725 m, and the sample flight takes off on
        // the Wallberg at 1,620 m.
        let lake = try XCTUnwrap(model.terrain.elevation(lat: 47.713, lon: 11.74))
        XCTAssertEqual(lake, 725, accuracy: 1)
        let takeoff = try XCTUnwrap(model.terrain.elevation(lat: 47.6642, lon: 11.7967))
        XCTAssertEqual(takeoff, 1620, accuracy: 5)

        // A session given this ground knows how high over it the glider
        // is, and has it in the side view.
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-terrain-test-\(UUID().uuidString)", isDirectory: true)
        let session = FlightSession(dataFolder: folder.path)
        session.useTerrain(terrain: model.terrain)
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let replay = try Replay(igcText: try String(contentsOf: sample, encoding: .utf8))
        _ = replay.advance(session: session, seconds: 540)

        let view = session.view(nowUnixS: replay.now())
        let above = try XCTUnwrap(view.heightAboveGroundM)
        XCTAssertEqual(above, 1155, accuracy: 40)
        let ahead = try XCTUnwrap(session.airspaceAhead(nowUnixS: replay.now()))
        XCTAssertEqual(ahead.ground.count, 101)
        // Nothing sounds like "clear" when the app has no airspace.
        XCTAssertEqual(ahead.words, "No airspace in the app")
        let ground = try XCTUnwrap(ahead.groundWords)
        XCTAssertTrue(ground.hasPrefix("Your line meets the ground in "), ground)
        XCTAssertNotNil(ahead.lineMeetsGroundM)
    }

    func testTilesAreAskedOfTheServerByTheirAddressAndARefusalDoesNoHarm() async throws {
        let asked = Asked()
        let model = TerrainModel(
            source: .server("https://tiles.example.org/{z}/{x}/{y}.webp"),
            folder: nil,
            transport: { address in
                await asked.note(address)
                return (404, Data())
            }
        )

        await model.load(around: Coordinate(lat: 47.6642, lon: 11.7967))

        // Five by five tiles of zoom 12, the one over the place first.
        let addresses = await asked.addresses
        XCTAssertEqual(addresses.count, 25)
        XCTAssertEqual(addresses.first?.absoluteString, "https://tiles.example.org/12/2182/1429.webp")
        XCTAssertEqual(model.terrain.tileCount(), 0)
        XCTAssertNil(model.terrain.elevation(lat: 47.6642, lon: 11.7967))
    }
}
