import VentiCore
import XCTest

@testable import Venti

/// What the app adds to the core's air around and above a take-off: how
/// the two figures of an hour are worded from the core's fields, how an
/// hour is read out, and which part of a layer the picture reaches. Which
/// wind is much stronger, where a layer lies and every sentence about
/// either are the core's and are tested there.
final class AirAroundTests: XCTestCase {
    private func hour(nearby: OutlookNearby? = nil, layers: [OutlookLayer] = []) -> OutlookHour {
        OutlookHour(
            timeUnixS: 1_795_352_400, hour: "14", verdict: .flyable, why: "", windFromDeg: 270, windKmh: "18",
            thermals: nil, climbMs: nil, thermalTopM: nil, thermalCloud: false, winds: [],
            layers: layers, nearby: nearby
        )
    }

    private let foehn = OutlookNearby(
        windKmh: "28", windWhere: "20 km to the south", windGroundM: 2400,
        gustKmh: "55", gustWhere: "in the square around the place", gustGroundM: nil,
        stronger: true, line: "Gusts to 55 km/h in the square around the place, 18 km/h at the place"
    )

    private let stable = OutlookLayer(
        bottomM: 1500, topM: 3100, inversion: false, fallCPer100m: 0.3,
        words: "Stable layer between 1500 and 3100 m: 0.3 °C per 100 m"
    )

    func testTheTwoFiguresSayWhereAndOverWhichGround() {
        XCTAssertEqual(AirAround.figures(foehn), [
            AirAround.Figure(name: "Wind near by", words: "28 km/h 20 km to the south over ground at 2400 m"),
            // Without the ground's height the figure ends with the place.
            AirAround.Figure(name: "Gusts near by", words: "55 km/h in the square around the place"),
        ])
        // An hour without a neighbourhood has no figures, and a figure the
        // model's squares do not have is left out.
        XCTAssertEqual(AirAround.figures(nil), [])
        let onlyWind = OutlookNearby(
            windKmh: "12", windWhere: "", windGroundM: nil, gustKmh: "", gustWhere: "", gustGroundM: nil,
            stronger: false, line: ""
        )
        XCTAssertEqual(AirAround.figures(onlyWind), [AirAround.Figure(name: "Wind near by", words: "12 km/h")])
    }

    func testAnHourIsReadOutLineFirstThenLayersThenFigures() {
        XCTAssertEqual(
            AirAround.spoken(hour(nearby: foehn, layers: [stable])),
            "Gusts to 55 km/h in the square around the place, 18 km/h at the place. "
                + "Stable layer between 1500 and 3100 m: 0.3 °C per 100 m. "
                + "Wind near by: 28 km/h 20 km to the south over ground at 2400 m. "
                + "Gusts near by: 55 km/h in the square around the place"
        )
        XCTAssertEqual(AirAround.clock(hour()), "14:00")
        // An hour with none of them says nothing and has no detail.
        XCTAssertEqual(AirAround.spoken(hour()), "")
        XCTAssertFalse(AirAround.hasDetail(hour()))
        XCTAssertTrue(AirAround.hasDetail(hour(layers: [stable])))
        XCTAssertTrue(AirAround.hasDetail(hour(nearby: foehn)))
    }

    func testALayerIsCutWhereThePictureEnds() throws {
        // A picture from 1000 m to 3000 m.
        let scale = AirScale(groundM: 1266, takeoffM: nil, tops: [])
        XCTAssertEqual(scale.floorM, 1000)
        XCTAssertEqual(scale.ceilingM, 3000)
        // A layer inside it is whole.
        let inside = try XCTUnwrap(scale.within(bottomM: 1500, topM: 2000))
        XCTAssertEqual(inside.bottomM, 1500)
        XCTAssertEqual(inside.topM, 2000)
        // One that goes on above is cut at the ceiling, one that begins
        // under the floor at the floor.
        let above = try XCTUnwrap(scale.within(bottomM: 1500, topM: 4200))
        XCTAssertEqual(above.topM, 3000)
        let under = try XCTUnwrap(scale.within(bottomM: 800, topM: 1200))
        XCTAssertEqual(under.bottomM, 1000)
        // One wholly above the picture, and one without any height, are
        // not drawn.
        XCTAssertNil(scale.within(bottomM: 3200, topM: 4500))
        XCTAssertNil(scale.within(bottomM: 2000, topM: 2000))
    }
}
