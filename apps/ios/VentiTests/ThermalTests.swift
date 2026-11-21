import XCTest
@testable import Venti

/// What the app adds to the core's thermal forecast: which step of the
/// design pack's scale a climb is drawn in, and how the picture of a
/// day's air lays out its heights. The forecast itself is the core's and
/// is tested there.
final class ThermalTests: XCTestCase {
    func testAClimbIsDrawnInTheStepOfThePacksThermalScale() {
        // The pack's breaks: 0.5, 1, 1.5, 2 and 3 m/s.
        XCTAssertEqual(ThermalBand(metresPerSecond: 0).step, 0)
        XCTAssertEqual(ThermalBand(metresPerSecond: 0.4).step, 0)
        XCTAssertEqual(ThermalBand(metresPerSecond: 0.5).step, 1)
        XCTAssertEqual(ThermalBand(metresPerSecond: 1.2).step, 2)
        XCTAssertEqual(ThermalBand(metresPerSecond: 1.9).step, 3)
        XCTAssertEqual(ThermalBand(metresPerSecond: 2.3).step, 4)
        XCTAssertEqual(ThermalBand(metresPerSecond: 3).step, 5)
        XCTAssertEqual(ThermalBand(metresPerSecond: 7).step, 5)
    }

    func testThePictureOfTheAirHoldsTheGroundTheTakeoffAndTheThermals() {
        // A model's ground at 1266 m, a take-off at 1620 m, thermals to
        // 3500 m: from 1000 m to 4000 m, a line every 500 m.
        let scale = AirScale(groundM: 1266, takeoffM: 1620, tops: [3400, 3500, 3450])
        XCTAssertEqual(scale.floorM, 1000)
        XCTAssertEqual(scale.ceilingM, 4000)
        XCTAssertEqual(scale.lines, [1000, 1500, 2000, 2500, 3000, 3500, 4000])
        // The floor is at the foot of the picture and the ceiling at its head.
        XCTAssertEqual(scale.y(1000, in: 300), 300, accuracy: 0.001)
        XCTAssertEqual(scale.y(4000, in: 300), 0, accuracy: 0.001)
        XCTAssertEqual(scale.y(2500, in: 300), 150, accuracy: 0.001)
    }

    func testADayWithoutThermalsStillHasRoomForTheWindOverTheTakeoff() {
        // No thermals: a kilometre over the take-off, and the round
        // figure above that.
        let scale = AirScale(groundM: 812, takeoffM: 1550, tops: [])
        XCTAssertEqual(scale.floorM, 500)
        XCTAssertEqual(scale.ceilingM, 3500)
        // A take-off under the model's ground sets the floor.
        let valley = AirScale(groundM: 1266, takeoffM: 780, tops: [])
        XCTAssertEqual(valley.floorM, 500)
        // Without a take-off's height the model's ground stands for it.
        let unknown = AirScale(groundM: 1266, takeoffM: nil, tops: [])
        XCTAssertEqual(unknown.floorM, 1000)
        XCTAssertEqual(unknown.ceilingM, 3000)
        // The model's next level above that kilometre is in the picture:
        // over a ground at 1490 m there is none between 1670 m and 3010 m.
        let high = AirScale(groundM: 1490, takeoffM: nil, tops: [], levels: [1500, 1570, 1670, 3010, 4180])
        XCTAssertEqual(high.floorM, 1000)
        XCTAssertEqual(high.ceilingM, 4000)
        // A picture that spans more than 3000 m has a line every 1000 m.
        let deep = AirScale(groundM: 400, takeoffM: nil, tops: [4200])
        XCTAssertEqual(deep.lines, [0, 1000, 2000, 3000, 4000, 5000])
    }
}
