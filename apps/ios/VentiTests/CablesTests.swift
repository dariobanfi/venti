import MapLibre
import VentiCore
import XCTest

@testable import Venti

/// The cables' plumbing: the core's request is sent, its answer read into
/// a layer, asked for again only far from where it was, and handed to the
/// cockpit's session.
@MainActor
final class CablesTests: XCTestCase {
    /// One gondola and one power line without a height, in the server's
    /// answer's form.
    static let answer = Data(
        """
        {"layers": [{"source": "osm-cables", "fetched_at": "2026-11-18T19:05:00Z", "as_of": "2026-11-18T19:05:00Z",
          "covers": [{"south": 47.66, "west": 11.7, "north": 47.74, "east": 11.82}],
          "cables": [
            {"source": "osm-cables", "local": "way/19038453", "kind": "gondola",
             "line": [{"lat": 47.6750127, "lon": 11.7767441}, {"lat": 47.6614147, "lon": 11.793787}],
             "url": "https://www.openstreetmap.org/way/19038453"},
            {"source": "osm-cables", "local": "way/37016046", "kind": "power-line",
             "line": [{"lat": 47.7767494, "lon": 11.6832523}, {"lat": 47.7229026, "lon": 11.711016}],
             "voltage_kv": 110.0, "url": "https://www.openstreetmap.org/way/37016046"}]}],
         "attributions": [{"source": "osm-cables", "provider": "OpenStreetMap contributors", "licence": "ODbL 1.0",
           "text": "© OpenStreetMap contributors", "url": "https://www.openstreetmap.org/copyright"}]}
        """.utf8
    )

    func testTheCablesAreAskedForReadAndAskedForAgainOnlyFarAway() async throws {
        var asked: [URL] = []
        let model = CablesModel { url in
            asked.append(url)
            return (200, Self.answer)
        }
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8090"))
        model.server = { server }
        let place = Coordinate(lat: 47.67, lon: 11.78)

        await model.refresh(around: place)
        XCTAssertEqual(asked.count, 1)
        XCTAssertEqual(asked.first?.path, try cablesRequest(lat: place.lat, lon: place.lon).path.components(separatedBy: "?").first)
        XCTAssertEqual(model.lines.count, 2)
        XCTAssertTrue(model.lines.contains { $0.words.hasPrefix("Gondola") })
        XCTAssertEqual(model.lines.filter(\.heightMapped).count, 0)
        XCTAssertEqual(model.creditWords, "Cables: OpenStreetMap contributors")
        XCTAssertFalse(model.layer?.limits.isEmpty ?? true)

        // A few kilometres on: what is held still covers it.
        await model.refresh(around: Coordinate(lat: 47.70, lon: 11.80))
        XCTAssertEqual(asked.count, 1)
        // Twenty kilometres on: asked for again.
        await model.refresh(around: Coordinate(lat: 47.85, lon: 11.78))
        XCTAssertEqual(asked.count, 2)
    }

    func testTheMapDrawsAMappedCableSolidAndAnUnmappedOneDashed() throws {
        let set = try CableSet.fromAnswer(status: 200, body: Self.answer)
        let features = set.layer().lines.compactMap(CablesLayers.feature)
        XCTAssertEqual(features.count, 2)
        XCTAssertEqual(features.compactMap { $0.attribute(forKey: "mapped") as? Int }, [0, 0])
        XCTAssertEqual(
            Set(features.compactMap { $0.attribute(forKey: "id") as? String }), Set(set.layer().lines.map(\.id))
        )
    }

    func testTheTaskPageComesOnlyWithATask() {
        XCTAssertEqual(CockpitPage.flight.next(hasTask: true), .task)
        XCTAssertEqual(CockpitPage.flight.next(hasTask: false), .instruments)
        XCTAssertEqual(CockpitPage.task.next(hasTask: true), .instruments)
        XCTAssertEqual(CockpitPage.map.next(hasTask: true), .flight)
        // A swipe the other way goes back the same way round.
        for hasTask in [true, false] {
            for page in CockpitPage.all(hasTask: hasTask) {
                XCTAssertEqual(page.next(hasTask: hasTask).previous(hasTask: hasTask), page)
            }
        }
        XCTAssertEqual(CockpitPage.instruments.previous(hasTask: true), .task)
        XCTAssertEqual(CockpitPage.instruments.previous(hasTask: false), .flight)
        XCTAssertEqual(CockpitPage.all(hasTask: false), [.instruments, .map, .flight])
    }

    func testThePageFollowsTheFingerAndTurnsWhenLetGoFarOrFlung() {
        // The page starts where the finger was when the drag was first
        // seen, and follows it both ways.
        XCTAssertEqual(PageSlide.offset(following: 20, from: 32), 0)
        XCTAssertEqual(PageSlide.offset(following: -100, from: 32), -68)
        XCTAssertEqual(PageSlide.offset(following: 100, from: 32), 68)
        // Let go: far enough sideways turns it, a short one does not, and
        // one more up or down than sideways never does.
        XCTAssertTrue(PageSlide.turns(sideways: -60, upOrDown: 10, flung: -60, across: 390, atLeast: 48))
        XCTAssertFalse(PageSlide.turns(sideways: -30, upOrDown: 0, flung: -40, across: 390, atLeast: 48))
        XCTAssertFalse(PageSlide.turns(sideways: -60, upOrDown: 50, flung: -300, across: 390, atLeast: 48))
        // A short flick that would have gone half the page along turns it;
        // one flung back the other way does not.
        XCTAssertTrue(PageSlide.turns(sideways: 30, upOrDown: 0, flung: 220, across: 390, atLeast: 48))
        XCTAssertFalse(PageSlide.turns(sideways: 30, upOrDown: 0, flung: -220, across: 390, atLeast: 48))
    }
}
