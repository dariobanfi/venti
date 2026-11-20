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

        // And how far it glides every way: a ring around the glider that
        // lies nearer towards the mountain than out over the lake.
        let range = try XCTUnwrap(session.glideRange(nowUnixS: replay.now()))
        XCTAssertEqual(range.outline.count, 73)
        XCTAssertEqual(range.outline.first, range.outline.last)
        XCTAssertEqual(range.reserveM, 100)
        XCTAssertLessThan(range.nearestM, 3000)
        XCTAssertGreaterThan(range.farthestM, 8000)
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

extension TerrainTests {
    func testTheGroundUnderAFlightIsReadForItsProfile() async throws {
        let region = try MapStyle.bundledRegion()
        let model = TerrainModel(
            source: .archive(region.appendingPathComponent("terrain.pmtiles", isDirectory: false)),
            folder: nil,
            transport: { _ in (500, Data()) }
        )
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let flight = try flightFromFile(igcText: try String(contentsOf: sample, encoding: .utf8))

        let ground = await model.ground(under: flight)

        // At 240 moments, from the take-off on the Wallberg at 1,620 m to
        // the valley floor.
        XCTAssertEqual(ground.count, 240)
        XCTAssertEqual(try XCTUnwrap(ground.first).elevationM, 1620, accuracy: 10)
        XCTAssertLessThan(try XCTUnwrap(ground.last).elevationM, 800)

        // The chart makes room for the lowest ground, and the panel reads
        // the height over the ground at any moment of the track.
        let range = AltitudeChart.altitudeRange(of: flight, over: ground)
        XCTAssertLessThanOrEqual(range.lowerBound, 700)
        XCTAssertEqual(AltitudeChart.altitudeRange(of: flight).lowerBound, 700, accuracy: 100)
        let middle = flight.track[flight.track.count / 2]
        let above = try XCTUnwrap(FlightPanel.heightAboveGround(at: middle.seconds, in: ground))
        XCTAssertGreaterThan(above, 500)
        XCTAssertNil(FlightPanel.heightAboveGround(at: middle.seconds, in: []))
        // Where the ground was read at two moments far apart only, a
        // moment between them has none.
        let sparse = [try XCTUnwrap(ground.first), try XCTUnwrap(ground.last)]
        XCTAssertNotNil(FlightPanel.heightAboveGround(at: 0, in: sparse))

        // Without a source there is no ground, and nothing is asked.
        let none = TerrainModel(source: .nowhere, folder: nil, transport: { _ in
            XCTFail("Nothing is fetched without a source")
            return (500, Data())
        })
        let nothing = await none.ground(under: flight)
        XCTAssertTrue(nothing.isEmpty)
    }

    func testAKeptRegionGivesTheGroundFromItsFileWithoutANetwork() async throws {
        let archive = try MapStyle.bundledRegion().appendingPathComponent("terrain.pmtiles", isDirectory: false)
        let model = TerrainModel(source: .nowhere, folder: nil, transport: { _ in
            XCTFail("Nothing is fetched where a kept region has the ground")
            return (500, Data())
        })
        let wallberg = Coordinate(lat: 47.6642, lon: 11.7967)

        // Without the region there is no ground to be had.
        await model.load(around: wallberg)
        XCTAssertNil(model.terrain.elevation(lat: 47.6642, lon: 11.7967))

        // With it, the tiles around the glider are read from its file.
        model.use(archives: [archive.path])
        XCTAssertNil(model.terrain.elevation(lat: 47.6642, lon: 11.7967))
        await model.load(around: wallberg)
        let takeoff = try XCTUnwrap(model.terrain.elevation(lat: 47.6642, lon: 11.7967))
        XCTAssertEqual(takeoff, 1620, accuracy: 5)

        // So is the ground under a flight, at the zoom level its extent
        // asks for.
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let flight = try flightFromFile(igcText: try String(contentsOf: sample, encoding: .utf8))
        let ground = await model.ground(under: flight)
        XCTAssertGreaterThan(ground.count, 100)
        XCTAssertEqual(try XCTUnwrap(ground.first).elevationM, 1620, accuracy: 25)

        // A region that is deleted is let go; a file that is no archive
        // is left out and takes nothing else with it.
        model.use(archives: ["/nowhere/terrain.pmtiles"])
        let fresh = await model.ground(under: flight)
        XCTAssertTrue(fresh.isEmpty)
    }
}
