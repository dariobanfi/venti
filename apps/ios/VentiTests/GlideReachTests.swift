import VentiCore
import XCTest

@testable import Venti

/// What a glide reaches from a place on the map: the model over the real
/// core and the ground that comes with the app, and what the map draws.
@MainActor
final class GlideReachTests: XCTestCase {
    private let wallberg = Coordinate(lat: 47.6642, lon: 11.7967)

    /// The ground that comes with the app, and how often it was asked for.
    @MainActor
    private final class Ground {
        var asked: [Bounds] = []
        let model: TerrainModel

        init() throws {
            let region = try MapStyle.bundledRegion()
            model = TerrainModel(
                source: .archive(region.appendingPathComponent("terrain.pmtiles", isDirectory: false)),
                folder: nil,
                transport: { _ in (500, Data()) }
            )
        }

        func bring(_ area: Bounds) async -> Terrain {
            asked.append(area)
            return await model.ground(covering: area)
        }
    }

    func testTheReachFromTheWallbergIsTheCoresAndASettingAsksAgainWithoutNewGround() async throws {
        let ground = try Ground()
        let reach = GlideReachModel(ground: { area in await ground.bring(area) })
        reach.start(from: wallberg, glideRatio: 8.2)
        // The pilot's glide, within the steps.
        XCTAssertEqual(reach.glideRatio, 8.0)
        XCTAssertNil(reach.setting?.startHeightM)
        XCTAssertNil(reach.setting?.windKmh)

        await reach.ask()
        let answer = try XCTUnwrap(reach.answer)
        XCTAssertTrue(answer.words.hasPrefix("From"), answer.words)
        XCTAssertFalse(answer.note.isEmpty)
        XCTAssertEqual(answer.outline.count, answer.rays.count + 1)
        XCTAssertFalse(reach.isWorking)
        // The first box reaches 10 km each way; a wider one only while a
        // direction runs off the ground, up to 80 km.
        let first = try XCTUnwrap(ground.asked.first)
        XCTAssertEqual((first.north - first.south) / 2 * 111.32, 10, accuracy: 0.01)
        XCTAssertLessThanOrEqual(ground.asked.count, 4)
        if answer.cutShort {
            XCTAssertEqual(ground.asked.count, 4)
        }

        let drawing = try XCTUnwrap(GlideReachDrawing(answer, from: wallberg))
        XCTAssertEqual(drawing.place, wallberg)
        XCTAssertFalse(drawing.edges.isEmpty && drawing.unloaded.isEmpty)

        // A better glide is asked of the core again over the same ground.
        let asked = ground.asked.count
        reach.glideRatio = 10
        await reach.ask()
        let better = try XCTUnwrap(reach.answer)
        XCTAssertEqual(better.glideRatio, 10)
        XCTAssertGreaterThanOrEqual(better.farthestM, answer.farthestM)
        XCTAssertEqual(ground.asked.count, asked)
        XCTAssertNotEqual(GlideReachDrawing(better, from: wallberg)?.id, drawing.id)

        // A wind is handed to the core only while it is on.
        reach.windIsOn = true
        XCTAssertEqual(reach.setting?.windFromDeg, 270)
        XCTAssertEqual(reach.setting?.windKmh, 15)
        reach.windIsOn = false
        XCTAssertNil(reach.setting?.windFromDeg)

        // The start height in steps of 50 m from the ground, and the ground
        // again a step down.
        let groundM = try XCTUnwrap(answer.groundM)
        reach.raiseStart()
        let raised = try XCTUnwrap(reach.startHeightM)
        XCTAssertGreaterThan(raised, groundM)
        XCTAssertLessThanOrEqual(raised - groundM, 50)
        XCTAssertEqual(raised.truncatingRemainder(dividingBy: 50), 0)
        reach.raiseStart()
        XCTAssertEqual(reach.startHeightM, raised + 50)
        reach.lowerStart()
        reach.lowerStart()
        XCTAssertNil(reach.startHeightM)

        // "Clear" takes it all away.
        reach.clear()
        XCTAssertNil(reach.place)
        XCTAssertNil(reach.answer)
        XCTAssertNil(reach.setting)
    }

    func testWithoutGroundTheCoreSaysSoAndNothingIsDrawn() async throws {
        let reach = GlideReachModel()
        reach.start(from: wallberg, glideRatio: 8)
        await reach.ask()
        let answer = try XCTUnwrap(reach.answer)
        XCTAssertTrue(answer.outline.isEmpty)
        XCTAssertFalse(answer.words.isEmpty)
        XCTAssertNil(GlideReachDrawing(answer, from: wallberg))
    }

    func testTheEdgeIsDashedWhereBothEndsLieWhereTheGroundLoadedEnds() {
        let ends: [ReachEnd] = [.ground, .notLoaded, .notLoaded, .ground]
        let rays = ends.enumerated().map { index, end in
            ReachRay(
                bearingDeg: Double(index) * 90, distanceM: 1000,
                endAt: Coordinate(lat: Double(index), lon: 0), arrivalM: 1000, overGroundM: nil, end: end
            )
        }
        let stretches = GlideReachDrawing.stretches(rays)
        XCTAssertEqual(stretches.map(\.isUnloaded), [false, true, false])
        XCTAssertEqual(stretches.map { $0.points.map(\.lat) }, [[0, 1], [1, 2], [2, 3, 0]])
    }

    func testTheGlideIsHeldWithinItsSteps() {
        XCTAssertEqual(GlideReachSteps.held(8.3), 8.5)
        XCTAssertEqual(GlideReachSteps.held(2), 4)
        XCTAssertEqual(GlideReachSteps.held(20), 14)
        let box = GlideReachSteps.box(around: wallberg, radiusKm: 10)
        XCTAssertEqual(box.north - wallberg.lat, wallberg.lat - box.south, accuracy: 1e-9)
        XCTAssertGreaterThan(box.east - box.west, box.north - box.south)
    }
}
