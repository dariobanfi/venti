import VentiCore
import XCTest

@testable import Venti

/// The cockpit's session model against the real core, fed by a replay.
@MainActor
final class FlySessionTests: XCTestCase {
    private func makeFolder() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-fly-test-\(UUID().uuidString)", isDirectory: true)
    }

    private func sampleText() throws -> String {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    func testAReplayDrivesTheCockpitAndSavesNothing() async throws {
        let folder = makeFolder()
        let fly = FlySession(dataFolder: folder)
        XCTAssertNil(fly.mode)
        XCTAssertNil(fly.view)

        // Five hundred times faster: five minutes of flight in under a second.
        fly.startReplay(igcText: try sampleText(), speed: 500)
        XCTAssertEqual(fly.mode, .replay(speed: 500))
        XCTAssertNotNil(fly.view)

        try await Task.sleep(for: .milliseconds(1200))

        let view = try XCTUnwrap(fly.view)
        XCTAssertEqual(view.phase, .flying)
        XCTAssertFalse(view.recording)
        XCTAssertNotNil(view.altitudeM)
        XCTAssertNotNil(view.climbMs)
        XCTAssertFalse(view.trail.isEmpty)
        // The thermal it climbed in and left is marked for the map, with
        // its climb.
        XCTAssertEqual(view.thermals.count, 1)
        XCTAssertTrue(view.thermals.first?.label.hasPrefix("+2.") ?? false, "\(view.thermals)")

        XCTAssertNil(fly.finish())
        XCTAssertNil(fly.mode)
        XCTAssertNil(fly.view)
        XCTAssertTrue(pendingRecordings(dataFolder: folder.path).isEmpty)
    }

    func testAFinishedRecordingGoesIntoTheLogbookAndIsCleared() async throws {
        let folder = makeFolder()
        let logbook = try LogbookModel.open(folder: folder)

        // Record the first two minutes of the sample through the core, as a
        // flight would be.
        let session = FlightSession(dataFolder: folder.path)
        try session.startRecording(pilot: "Test Pilot", glider: nil)
        let replay = try Replay(igcText: try sampleText())
        _ = replay.advance(session: session, seconds: 120)
        let recording = try XCTUnwrap(try session.stopRecording())
        XCTAssertEqual(pendingRecordings(dataFolder: folder.path).count, 1)

        let id = try await logbook.adopt(recording)

        XCTAssertEqual(logbook.view.totals.flights, 1)
        XCTAssertEqual(logbook.view.years.first?.flights.first?.id, id)
        XCTAssertEqual(logbook.view.totals.airtimeS, 120)
        let flight = try await logbook.flight(id: id)
        XCTAssertEqual(flight.pilot, "Test Pilot")

        // The flight can be handed back out as a file with a recorder-style name.
        let exported = try await logbook.exportFile(id: id)
        XCTAssertEqual(exported.lastPathComponent, "2026-07-04-XVE-001-01.igc")
        XCTAssertEqual(try String(contentsOf: exported, encoding: .utf8), recording.igcText)
        // And read back as it is kept, for replaying it in the cockpit.
        let kept = try await logbook.fileText(id: id)
        XCTAssertEqual(kept, recording.igcText)
        XCTAssertTrue(pendingRecordings(dataFolder: folder.path).isEmpty)
    }

    func testARecordingLeftBehindIsAdoptedAtTheNextLaunch() async throws {
        let folder = makeFolder()
        let session = FlightSession(dataFolder: folder.path)
        try session.startRecording(pilot: nil, glider: nil)
        _ = try Replay(igcText: try sampleText()).advance(session: session, seconds: 60)
        // The app stops here without finishing the flight.

        let logbook = try LogbookModel.open(folder: folder)
        XCTAssertEqual(logbook.view.totals.flights, 0)
        await logbook.adoptInterruptedRecordings()

        XCTAssertEqual(logbook.view.totals.flights, 1)
        XCTAssertTrue(pendingRecordings(dataFolder: folder.path).isEmpty)
    }
}
