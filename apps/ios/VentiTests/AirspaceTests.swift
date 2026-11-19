import VentiCore
import XCTest

@testable import Venti

/// Airspace files and the cockpit's warnings against the real core.
@MainActor
final class AirspaceTests: XCTestCase {
    private func makeFolder() throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-airspace-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    func testAFileIsTakenInDrawnAndRemoved() async throws {
        let folder = try makeFolder()
        let airspace = AirspaceModel(core: try Core(dataFolder: folder.appendingPathComponent("data").path))
        XCTAssertTrue(airspace.isEmpty)
        XCTAssertNil(airspace.set)

        // The sample zone, and an airspace without a shape, which cannot
        // be read and must be reported.
        let text = LaunchOptions.sampleAirspace + "\n\nAC C\nAN NO SHAPE\nAL GND\nAH FL 100\n"
        let file = folder.appendingPathComponent("My Zones.txt")
        try text.write(to: file, atomically: true, encoding: .utf8)
        let notes = folder.appendingPathComponent("notes.txt")
        try "hello".write(to: notes, atomically: true, encoding: .utf8)

        let notice = await airspace.importFiles(at: [file, notes])
        XCTAssertTrue(notice.text.contains("My Zones.txt: 1 airspace read."), notice.text)
        XCTAssertTrue(notice.text.contains("1 airspace could not be read and is left out: you will not be warned about it."), notice.text)
        XCTAssertTrue(notice.text.contains("NO SHAPE"), notice.text)
        XCTAssertTrue(notice.text.contains("notes.txt: This is not an airspace file"), notice.text)

        XCTAssertEqual(airspace.files.map(\.name), ["my-zones"])
        XCTAssertEqual(airspace.files.first?.leftOut, 1)
        XCTAssertEqual(airspace.set?.count(), 1)

        let tegernsee = Bounds(south: 47.6, west: 11.6, north: 47.8, east: 11.9)
        let shapes = airspace.shapes(in: tegernsee)
        XCTAssertEqual(shapes.map(\.name), ["SAMPLE ZONE (NOT REAL)"])
        XCTAssertEqual(shapes.first?.policy, .avoid)
        XCTAssertEqual(
            AirspaceLayers.label(for: try XCTUnwrap(shapes.first)), "SAMPLE ZONE (NOT REAL)\nGND – 9000 ft AMSL"
        )
        XCTAssertTrue(airspace.shapes(in: Bounds(south: 40, west: 0, north: 41, east: 1)).isEmpty)

        airspace.delete(try XCTUnwrap(airspace.files.first))
        XCTAssertTrue(airspace.isEmpty)
        XCTAssertNil(airspace.set)
    }

    func testTheCockpitWarnsNearTheZoneAndTheWarningCanBeQuieted() async throws {
        let folder = try makeFolder()
        let core = try Core(dataFolder: folder.path)
        _ = try core.importAirspace(
            fileName: "zones.txt", bytes: Data(LaunchOptions.sampleAirspace.utf8), nowUnixS: 1_795_000_000
        )
        let set = try core.airspace()
        let fly = FlySession(dataFolder: folder)
        fly.airspace = { set }

        // Two thousand times faster: the whole flight in under a second.
        let sample = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        fly.startReplay(igcText: try String(contentsOf: sample, encoding: .utf8), speed: 2000)
        try await Task.sleep(for: .milliseconds(2500))

        // The flight ends beside the zone.
        let warning = try XCTUnwrap(fly.airspaceWarnings.first)
        XCTAssertEqual(warning.name, "SAMPLE ZONE (NOT REAL)")
        XCTAssertEqual(warning.detail, "Class D control zone · GND to 9000 ft AMSL")
        XCTAssertEqual(warning.source, "zones")
        XCTAssertFalse(warning.silenced)

        fly.acknowledgeAirspace(warning.id)
        XCTAssertEqual(fly.airspaceWarnings.first?.silenced, true)
        fly.unsilenceAirspace(warning.id)
        XCTAssertEqual(fly.airspaceWarnings.first?.silenced, false)
        fly.ignoreAirspaceToday(warning.id)
        XCTAssertEqual(fly.airspaceWarnings.first?.silenced, true)

        _ = fly.finish()
        XCTAssertTrue(fly.airspaceWarnings.isEmpty)
    }

    func testAnImportIsSummedUpForThePilot() {
        XCTAssertEqual(
            AirspaceModel.summary(
                of: AirspaceImport(airspaces: 312, leftOut: 0, assumed: 0, problems: []), file: "de.txt"
            ),
            "de.txt: 312 airspaces read."
        )
        let troubled = AirspaceModel.summary(
            of: AirspaceImport(airspaces: 10, leftOut: 2, assumed: 1, problems: ["Line 7 (X): no shape."]),
            file: "x.txt"
        )
        XCTAssertTrue(troubled.contains("2 airspaces could not be read and are left out: you will not be warned about them."))
        XCTAssertTrue(troubled.contains("In 1 place something could not be read and the cautious reading was taken."))
        XCTAssertTrue(troubled.hasSuffix("Line 7 (X): no shape."))
    }
}
