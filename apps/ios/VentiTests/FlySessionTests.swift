import CoreLocation
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

    func testAReplayCanBeHeldAndRunOn() async throws {
        let fly = FlySession(dataFolder: makeFolder())
        fly.startReplay(igcText: try sampleText(), speed: 200)
        try await Task.sleep(for: .milliseconds(700))

        // Held: the flight stands still, and the cockpit goes on showing it.
        fly.setReplaySpeed(0)
        XCTAssertEqual(fly.mode, .replay(speed: 0))
        // One refresh may have been under way when it was held.
        try await Task.sleep(for: .milliseconds(400))
        let held = try XCTUnwrap(fly.view?.flightTimeS)
        try await Task.sleep(for: .milliseconds(800))
        XCTAssertEqual(fly.view?.flightTimeS, held)
        XCTAssertGreaterThan(held, 60)

        // And on, at another speed.
        fly.setReplaySpeed(100)
        XCTAssertEqual(fly.mode, .replay(speed: 100))
        try await Task.sleep(for: .milliseconds(800))
        let later = try XCTUnwrap(fly.view?.flightTimeS)
        XCTAssertGreaterThan(later, held + 30)

        // A speed that is no speed changes nothing, and neither does any
        // once the replay is over.
        fly.setReplaySpeed(-1)
        fly.setReplaySpeed(.infinity)
        XCTAssertEqual(fly.mode, .replay(speed: 100))
        XCTAssertNil(fly.finish())
        fly.setReplaySpeed(20)
        XCTAssertNil(fly.mode)
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

    /// The cockpit flies by fixes of now: not by where the iPhone was when
    /// it last looked, and not by a guess from the phone network.
    func testOnlyAFixOfNowIsAPositionToFlyBy() {
        let now = Date()
        func location(accuracy: Double, age: TimeInterval) -> CLLocation {
            CLLocation(
                coordinate: CLLocationCoordinate2D(latitude: 47.664, longitude: 11.772),
                altitude: 1600, horizontalAccuracy: accuracy, verticalAccuracy: 10,
                timestamp: now.addingTimeInterval(-age)
            )
        }
        XCTAssertTrue(LocationProvider.isUsable(location(accuracy: 5, age: 1), now: now))
        XCTAssertTrue(LocationProvider.isUsable(location(accuracy: 100, age: 10), now: now))
        XCTAssertFalse(LocationProvider.isUsable(location(accuracy: 1_500, age: 1), now: now))
        XCTAssertFalse(LocationProvider.isUsable(location(accuracy: -1, age: 1), now: now))
        XCTAssertFalse(LocationProvider.isUsable(location(accuracy: 5, age: 600), now: now))
    }

    /// While the system's question is open, positions fail as if they
    /// were refused. Only a no is a no: the pilot is not told that Venti
    /// may not use the position before they have answered.
    func testAQuestionStillOpenIsNotARefusal() {
        XCTAssertFalse(LocationProvider.isRefused(.notDetermined))
        XCTAssertFalse(LocationProvider.isRefused(.authorizedWhenInUse))
        XCTAssertFalse(LocationProvider.isRefused(.authorizedAlways))
        XCTAssertTrue(LocationProvider.isRefused(.denied))
        XCTAssertTrue(LocationProvider.isRefused(.restricted))
    }
}
