import VentiCore
import XCTest

@testable import Venti

/// The thermal's picture in the cockpit: what the real core says while the
/// sample flight circles, and how the map draws it.
@MainActor
final class LiftTests: XCTestCase {
    func testTheSampleFlightsThermalIsPictured() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-lift-test-\(UUID().uuidString)", isDirectory: true)
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let session = FlightSession(dataFolder: folder.path)
        let replay = try Replay(igcText: try String(contentsOf: url, encoding: .utf8))

        // On the glide out there is no thermal to picture.
        _ = replay.advance(session: session, seconds: 60)
        XCTAssertNil(session.view(nowUnixS: replay.now()).lift)

        // Five minutes in, deep in the first climb: fifty seconds of
        // circles, moved with the westerly the circles themselves gave.
        _ = replay.advance(session: session, seconds: 240)
        let view = session.view(nowUnixS: replay.now())
        XCTAssertTrue(view.circling)
        let lift = try XCTUnwrap(view.lift)
        XCTAssertTrue(lift.drifted)
        XCTAssertEqual(lift.points.count, 51)
        let centre = try XCTUnwrap(lift.centre)
        XCTAssertEqual(centre.radiusM, 45, accuracy: 4)
        XCTAssertEqual(centre.outline.count, 49)
        XCTAssertFalse(centre.words.isEmpty)

        // A dot for each place, none smaller than the smallest or larger
        // than the largest.
        let dots = CockpitMap.Coordinator.liftFeatures(lift)
        XCTAssertEqual(dots.count, 51)
        for dot in dots {
            let radius = try XCTUnwrap(dot.attribute(forKey: "radius") as? Double)
            XCTAssertTrue((2...5).contains(radius))
            XCTAssertNotNil(dot.attribute(forKey: "band") as? Int)
        }
        XCTAssertTrue(CockpitMap.Coordinator.liftFeatures(nil).isEmpty)
    }

    func testAWindsAgeIsShownOnceItIsFiveMinutesOld() {
        XCTAssertNil(WindBadge.age(nil))
        XCTAssertNil(WindBadge.age(40))
        XCTAssertNil(WindBadge.age(299))
        XCTAssertEqual(WindBadge.age(300), "5 min")
        XCTAssertEqual(WindBadge.age(59 * 60 + 59), "59 min")
        XCTAssertEqual(WindBadge.age(2 * 3600 + 100), "2 h")
    }

    func testTheSampleFlightsWindIsNoStrongWindAndAgesOnTheGlide() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-lift-test-\(UUID().uuidString)", isDirectory: true)
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        let session = FlightSession(dataFolder: folder.path)
        let replay = try Replay(igcText: try String(contentsOf: url, encoding: .utf8))
        // Ten minutes in: on the glide, four minutes after the thermal
        // that gave the wind, which still stands at 11 km/h from the west.
        _ = replay.advance(session: session, seconds: 600)
        let view = session.view(nowUnixS: replay.now())
        let wind = try XCTUnwrap(view.wind)
        XCTAssertEqual(wind.speedMs, 3, accuracy: 0.5)
        XCTAssertEqual(wind.fromDeg, 270, accuracy: 8)
        XCTAssertEqual(try XCTUnwrap(view.windAgeS), 257, accuracy: 20)
        XCTAssertNil(view.strongWind)
    }

    func testADotGrowsWithTheLift() {
        XCTAssertEqual(CockpitMap.Coordinator.liftDotRadius(strength: 0), 2)
        XCTAssertEqual(CockpitMap.Coordinator.liftDotRadius(strength: 1), 5)
        XCTAssertEqual(CockpitMap.Coordinator.liftDotRadius(strength: 0.5), 3.5)
        // Whatever the core says, a dot stays a dot.
        XCTAssertEqual(CockpitMap.Coordinator.liftDotRadius(strength: 7), 5)
    }
}
