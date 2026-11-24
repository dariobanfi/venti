import VentiCore
import XCTest

@testable import Venti

/// The squares of the grid that the forecast by region stands in place
/// of: those whose middle lies in a region with a level, holes left out.
@MainActor
final class RegionShapesTests: XCTestCase {
    /// A square region from 11 to 12 E and 47 to 48 N with a hole in its
    /// middle, and a second region beside it as a polygon.
    private let outlines = """
        {"type":"FeatureCollection","features":[
          {"type":"Feature","properties":{"id":"A","name":"A"},"geometry":{"type":"MultiPolygon","coordinates":[
            [[[11,47],[12,47],[12,48],[11,48],[11,47]],[[11.4,47.4],[11.6,47.4],[11.6,47.6],[11.4,47.6],[11.4,47.4]]]
          ]}},
          {"type":"Feature","properties":{"id":"B","name":"B"},"geometry":{"type":"Polygon","coordinates":
            [[[12,47],[13,47],[13,48],[12,48],[12,47]]]
          }}
        ]}
        """

    private func square(_ lon: Double, _ lat: Double) -> RegionShapes.Box {
        RegionShapes.Box(south: lat - 0.05, west: lon - 0.05, north: lat + 0.05, east: lon + 0.05)
    }

    func testOnlyTheSquaresInTheRegionsNamedAreUnderThem() {
        let polygons = RegionShapes.polygons(of: outlines)
        XCTAssertEqual(Set(polygons.keys), ["A", "B"])
        let cells = [square(11.2, 47.2), square(11.5, 47.5), square(12.5, 47.5), square(10.5, 47.5)]
        // In A, in A's hole, in B, outside both.
        XCTAssertEqual(RegionShapes.cellsUnder(cells, polygons: polygons["A"] ?? []), [0])
        XCTAssertEqual(
            RegionShapes.cellsUnder(cells, polygons: (polygons["A"] ?? []) + (polygons["B"] ?? [])), [0, 2]
        )
        XCTAssertEqual(RegionShapes.cellsUnder(cells, polygons: []), [])
    }

    /// Each level takes its own token, worst to best.
    func testEachLevelHasItsOwnToken() {
        let levels: [RegionLevel] = [.notFlyable, .glidesOnly, .weak, .good, .veryGood]
        XCTAssertEqual(levels.map(\.tokenIndex), [0, 1, 2, 3, 4])
        XCTAssertEqual(regionLevels().map(\.level).reversed(), levels)
    }
}
