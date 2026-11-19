import VentiCore
import XCTest

@testable import Venti

/// The pilot's task on the iPhone: kept through the real core, shown,
/// handed on, and flown in a replay of the sample flight.
@MainActor
final class TaskTests: XCTestCase {
    private func makeFolder() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-task-test-\(UUID().uuidString)", isDirectory: true)
    }

    private func makeModel() throws -> TaskModel {
        TaskModel(core: try Core(dataFolder: makeFolder().path))
    }

    private func sampleText() throws -> String {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    func testATaskIsKeptShownHandedOnAndForgotten() throws {
        let tasks = try makeModel()
        XCTAssertNil(tasks.task)
        XCTAssertNil(tasks.file)

        // What is not a task is said in words and replaces nothing.
        XCTAssertFalse(tasks.keep("not a task"))
        XCTAssertEqual(tasks.problem, "This is not a task in the XCTSK format.")
        XCTAssertNil(tasks.task)

        XCTAssertTrue(tasks.keep(LaunchOptions.sampleTask))
        XCTAssertNil(tasks.problem)
        let task = try XCTUnwrap(tasks.task)
        XCTAssertEqual(
            task.turnpoints.map(\.name),
            ["Sample launch", "Sample start", "Sample turnpoint west", "Sample goal"]
        )
        XCTAssertEqual(
            task.turnpoints.map(\.detail),
            ["Launch", "Start · 1 km", "Turnpoint · 400 m", "Goal · 400 m"]
        )
        XCTAssertTrue(task.inALine.hasPrefix("4 turnpoints · "), task.inALine)
        // Out of the start, to the edge of the turnpoint 4.4 km from the
        // launch, and south to the edge of the goal: about 6 km.
        XCTAssertTrue((5_500...7_000).contains(task.lengthM), "\(task.lengthM)")
        XCTAssertTrue(TaskScreen.summary(of: task).contains("in the speed section"))
        XCTAssertEqual(TaskScreen.times(of: task).map(\.name), ["Launch open", "Race to goal", "Task ends"])

        // As a file and as a QR code it is the same task again.
        XCTAssertEqual(tasks.file?.name, "venti-task.xctsk")
        let code = try XCTUnwrap(tasks.code(compressed: true))
        XCTAssertTrue(code.hasPrefix("XCTSKZ:"))
        XCTAssertNotNil(TaskCodeSheet.image(for: code))
        XCTAssertTrue(try XCTUnwrap(tasks.code(compressed: false)).hasPrefix("XCTSK:"))
        let other = try makeModel()
        XCTAssertTrue(other.keep(code))
        XCTAssertEqual(other.task?.turnpoints.map(\.name), task.turnpoints.map(\.name))
        XCTAssertEqual(other.task?.lengthM ?? 0, task.lengthM, accuracy: 1)
        let file = try XCTUnwrap(try tasks.exportFile())
        other.forget()
        XCTAssertNil(other.task)
        XCTAssertTrue(other.keep(fileAt: file))
        XCTAssertEqual(other.task, task)

        tasks.forget()
        XCTAssertNil(tasks.task)
        XCTAssertNil(tasks.code(compressed: true))
    }

    func testLengthsAreWrittenTheWayTaskBoardsWriteThem() {
        XCTAssertEqual(TaskTurnpointView.length(400), "400 m")
        XCTAssertEqual(TaskTurnpointView.length(1000), "1 km")
        XCTAssertEqual(TaskTurnpointView.length(2500), "2.5 km")
        XCTAssertEqual(TaskTurnpointView.length(25_000), "25 km")
        XCTAssertEqual(TaskStrip.countdown(272), "4:32")
        XCTAssertEqual(TaskStrip.countdown(3700), "1:01:40")
    }

    func testTheSketchFitsTheWholeTaskNorthUp() throws {
        let tasks = try makeModel()
        tasks.keep(LaunchOptions.sampleTask)
        let task = try XCTUnwrap(tasks.task)
        let frame = TaskSketch.Frame(task: task, size: CGSize(width: 300, height: 200))
        for place in task.turnpoints.flatMap(\.outline) + task.way {
            let point = frame.point(place)
            XCTAssertTrue((0...300).contains(point.x) && (0...200).contains(point.y), "\(point)")
        }
        // The turnpoint lies north-west of the launch: up and to the left.
        let launch = frame.point(task.turnpoints[0].position)
        let west = frame.point(task.turnpoints[2].position)
        XCTAssertLessThan(west.x, launch.x)
        XCTAssertLessThan(west.y, launch.y)
    }

    func testTheNextTurnpointIsMarkedForTheMap() throws {
        let tasks = try makeModel()
        tasks.keep(LaunchOptions.sampleTask)
        let task = try XCTUnwrap(tasks.task)
        // The launch has no outline; the third turnpoint is the next.
        let drawn = CockpitMap.Coordinator.taskFeatures(task.turnpoints, next: 3)
        XCTAssertEqual(drawn.map { $0.attribute(forKey: "next") as? Int }, [0, 1, 0])
        XCTAssertEqual(drawn[1].pointCount, 73)
        XCTAssertTrue(CockpitMap.Coordinator.taskFeatures([], next: nil).isEmpty)
    }

    func testTheSampleFlightFliesTheSampleTask() async throws {
        let tasks = try makeModel()
        tasks.keep(LaunchOptions.sampleTask)
        let fly = FlySession(dataFolder: makeFolder())
        fly.taskFile = { tasks.file }

        // Two hundred times faster. The flight leaves the start cylinder
        // seven minutes in, five after its gate, and rounds the turnpoint
        // at eleven; it reaches the goal only at nineteen.
        fly.startReplay(igcText: try sampleText(), speed: 200)
        var waited = 0
        while fly.task?.reached[2] != true, waited < 150 {
            try await Task.sleep(for: .milliseconds(100))
            waited += 1
        }
        let status = try XCTUnwrap(fly.task)
        XCTAssertEqual(status.reached, [false, true, true, false])
        XCTAssertEqual(status.phase, .onTask)
        // The clock runs from the gate: 4 July 2026, 10:32 UTC.
        XCTAssertEqual(status.clockFromUnixS, 1_783_161_120)
        XCTAssertEqual(status.next?.name, "Sample goal")
        XCTAssertNil(status.gate)
        XCTAssertTrue(TaskStrip.headline(status).hasPrefix("On task "), TaskStrip.headline(status))
        XCTAssertTrue(TaskStrip.detail(status).hasPrefix("4 of 4"), TaskStrip.detail(status))
        XCTAssertTrue(TaskStrip.goalDetail(status).hasPrefix("to goal"))
        // The cockpit leads to the task's next turnpoint.
        XCTAssertEqual(fly.way?.name, "Sample goal")
        XCTAssertNil(fly.chosenPlace)

        // A place the pilot chooses comes first; letting go of it goes
        // back to the task.
        fly.goTo(TargetPlace(name: "Somewhere", position: Coordinate(lat: 47.7, lon: 11.8), elevationM: nil))
        XCTAssertEqual(fly.chosenPlace, "Somewhere")
        XCTAssertEqual(fly.way?.name, "Somewhere")
        fly.goTo(nil)
        XCTAssertNil(fly.chosenPlace)
        XCTAssertEqual(fly.way?.name, "Sample goal")

        // Stepping back by hand.
        fly.stepTask(forward: false)
        XCTAssertEqual(fly.task?.next?.name, "Sample turnpoint west")

        XCTAssertNil(fly.finish())
        XCTAssertNil(fly.task)
    }

    func testWithoutATaskTheCockpitHasNone() async throws {
        let fly = FlySession(dataFolder: makeFolder())
        fly.startReplay(igcText: try sampleText(), speed: 500)
        try await Task.sleep(for: .milliseconds(600))
        XCTAssertNotNil(fly.view)
        XCTAssertNil(fly.task)
        XCTAssertNil(fly.finish())
    }
}
