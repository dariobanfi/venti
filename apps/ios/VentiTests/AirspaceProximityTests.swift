import XCTest

@testable import Venti

/// The height line of the cockpit's large airspace reading: where the
/// glider and the airspace lie on one scale, from its foot (0) to its top
/// (1).
final class AirspaceProximityTests: XCTestCase {
    func testAFloorAboveTheGliderIsAboveItsMark() {
        let scale = AirspaceHeightScale(altitudeM: 1500, floorM: 1900, ceilingM: 2500)
        XCTAssertLessThan(scale.glider, scale.bottom)
        XCTAssertLessThan(scale.bottom, scale.top)
        XCTAssertFalse(scale.runsOffTheFoot)
        XCTAssertFalse(scale.runsOffTheTop)
        // A little air at both ends.
        XCTAssertGreaterThan(scale.glider, 0)
        XCTAssertLessThan(scale.top, 1)
    }

    func testACeilingBelowTheGliderIsBelowItsMark() {
        let scale = AirspaceHeightScale(altitudeM: 1500, floorM: 900, ceilingM: 1200)
        XCTAssertLessThan(scale.bottom, scale.top)
        XCTAssertLessThan(scale.top, scale.glider)
        XCTAssertFalse(scale.runsOffTheFoot)
    }

    func testInTheAirspaceTheMarkIsBetweenFloorAndCeiling() {
        let scale = AirspaceHeightScale(altitudeM: 1500, floorM: 1200, ceilingM: 1800)
        XCTAssertLessThan(scale.bottom, scale.glider)
        XCTAssertLessThan(scale.glider, scale.top)
        XCTAssertEqual(scale.glider, 0.5, accuracy: 0.001)
    }

    /// A floor at the ground without the ground's height is far below the
    /// glider (-500 m): it runs off the foot instead of squeezing the rest.
    func testAFloorFarBelowRunsOffTheFoot() {
        let scale = AirspaceHeightScale(altitudeM: 2000, floorM: -500, ceilingM: 2600)
        XCTAssertTrue(scale.runsOffTheFoot)
        XCTAssertEqual(scale.bottom, 0)
        XCTAssertGreaterThan(scale.glider, 0.1)
        XCTAssertLessThan(scale.glider, scale.top)
    }

    func testNoCeilingOrOneFarAboveRunsOffTheTop() {
        for ceiling in [nil, 12_000.0] {
            let scale = AirspaceHeightScale(altitudeM: 1500, floorM: 1800, ceilingM: ceiling)
            XCTAssertTrue(scale.runsOffTheTop)
            XCTAssertEqual(scale.top, 1)
            XCTAssertLessThan(scale.glider, scale.bottom)
        }
    }

    /// A few metres between the glider and the floor do not fill the line.
    func testTheLineSpansAtLeastItsLeastHeight() {
        let scale = AirspaceHeightScale(altitudeM: 1500, floorM: 1510, ceilingM: 1520)
        XCTAssertLessThan(scale.top - scale.glider, 0.1)
        for part in [scale.glider, scale.bottom, scale.top] {
            XCTAssertGreaterThan(part, 0.2)
            XCTAssertLessThan(part, 0.8)
        }
    }
}
