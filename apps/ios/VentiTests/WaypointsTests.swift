import VentiCore
import XCTest

@testable import Venti

/// Waypoints and navigation against the real core.
@MainActor
final class WaypointsTests: XCTestCase {
    private func makeFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-waypoints-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    private func write(_ text: String, named name: String, in folder: URL) throws -> URL {
        let url = folder.appendingPathComponent(name)
        try text.write(to: url, atomically: true, encoding: .utf8)
        return url
    }

    func testFilesAreImportedOnceAndWhatHappenedIsSaid() async throws {
        let folder = try makeFolder()
        let waypoints = WaypointsModel(core: try Core(dataFolder: folder.appendingPathComponent("data").path))
        XCTAssertTrue(waypoints.all.isEmpty)

        let file = try write(LaunchOptions.sampleWaypoints, named: "sample.cup", in: folder)
        let first = await waypoints.importFiles(at: [file])
        XCTAssertEqual(first, ImportNotice(title: "3 waypoints added", text: ""))
        XCTAssertEqual(
            waypoints.all.map(\.name), ["Sample landing field", "Sample summit far away", "Sample turnpoint"]
        )

        // The same file again, a GEO file with one line that cannot be
        // read, and a file that holds no waypoints at all.
        let geo = try write(
            "$FormatGEO\nB01123    N 46 14 18.84    E 013 34 38.16  1230  Kobala\nB02 N 46 14\n",
            named: "task.wpt", in: folder
        )
        let other = try write("hello", named: "notes.txt", in: folder)
        let second = await waypoints.importFiles(at: [file, geo, other])
        XCTAssertEqual(second.title, "1 waypoint added")
        XCTAssertTrue(second.text.contains("3 were already there."), second.text)
        XCTAssertTrue(second.text.contains("task.wpt: 1 line could not be read. Line 3:"), second.text)
        XCTAssertTrue(second.text.contains("notes.txt: This is not a waypoint file"), second.text)
        XCTAssertEqual(waypoints.all.count, 4)

        // Nearest first from the sample flight's take-off, with distances.
        let near = waypoints.near(Coordinate(lat: 47.6642, lon: 11.7967))
        XCTAssertEqual(near.first?.name, "Sample landing field")
        XCTAssertEqual(near.first?.kind, .landing)
        XCTAssertEqual(near.first?.detail, "3.8 km NW · 735 m · SLF")
        XCTAssertEqual(near.last?.name, "Kobala")

        // Out again as a file other programs read, and gone when deleted.
        let exported = try waypoints.exportFile()
        XCTAssertEqual(exported.lastPathComponent, "venti-waypoints.cup")
        XCTAssertTrue(try String(contentsOf: exported, encoding: .utf8).contains("\"Sample landing field\""))
        waypoints.delete(id: try XCTUnwrap(near.first?.id))
        XCTAssertEqual(waypoints.all.count, 3)
        waypoints.deleteAll()
        XCTAssertTrue(waypoints.all.isEmpty)
    }

    func testTheCockpitNavigatesToTheTakeOffUntilAPlaceIsChosen() async throws {
        let folder = try makeFolder()
        let fly = FlySession(dataFolder: folder)
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        fly.startReplay(igcText: try String(contentsOf: sample, encoding: .utf8), speed: 500)
        try await Task.sleep(for: .milliseconds(1200))

        let home = try XCTUnwrap(fly.way)
        XCTAssertTrue(home.isTakeoff)
        XCTAssertEqual(home.name, "Take-off")

        fly.goTo(
            TargetPlace(name: "Sample landing field", position: Coordinate(lat: 47.6833, lon: 11.755), elevationM: 735)
        )
        let way = try XCTUnwrap(fly.way)
        XCTAssertFalse(way.isTakeoff)
        XCTAssertEqual(way.name, "Sample landing field")
        XCTAssertGreaterThan(way.distanceM, 500)
        XCTAssertNotNil(way.arrivalHeightM)
        XCTAssertNotNil(way.requiredGlide)

        fly.goTo(nil)
        XCTAssertEqual(fly.way?.isTakeoff, true)

        _ = fly.finish()
        XCTAssertNil(fly.way)
    }
}
