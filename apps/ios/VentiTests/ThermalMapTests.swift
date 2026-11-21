import VentiCore
import XCTest

@testable import Venti

/// What a Venti server answers for the forecast map: four squares in a
/// row on three days, the first of which is over by the time asked.
private enum ServerThermals {
    static let answer = """
    {
        "source": "open-meteo", "model": "icon-d2",
        "run": "2026-07-12T03:00:00Z", "computed_at": "2026-07-12T04:41:00Z",
        "grid": {"south": 47.0, "west": 11.0, "step_deg": 0.1, "rows": 1, "columns": 4},
        "ground_m": [612, 1240, null, 900],
        "days": [
            {"date": "2026-07-11", "whole": true,
             "verdict": ["flyable", "flyable", "flyable", "flyable"],
             "thermals": ["local", "local", "local", "local"],
             "climb_ms": [0.7, 0.7, 0.7, 0.7], "top_m": [1800, 1800, 1800, 1800],
             "cloud": [false, false, false, false]},
            {"date": "2026-07-12", "whole": false,
             "verdict": ["flyable", "not_flyable", null, "marginal"],
             "why": [null, "rain", null, "gusts"],
             "thermals": ["cross_country", "big", null, null],
             "climb_ms": [1.8, 2.4, null, null], "top_m": [3250, 3400, null, null],
             "cloud": [true, false, false, false]},
            {"date": "2026-07-13", "whole": true,
             "verdict": ["flyable", "flyable", "flyable", "flyable"],
             "thermals": ["big", "local", "weak", "big"],
             "climb_ms": [2.2, 0.6, 0.3, 2.6], "top_m": [3400, 2100, 1800, 3600],
             "cloud": [true, true, false, false]}
        ],
        "attributions": [{"source": "open-meteo", "provider": "Open-Meteo",
            "licence": "CC BY 4.0", "text": "Weather data by Open-Meteo.com",
            "url": "https://open-meteo.com/"}]
    }
    """

    /// Noon on 12 July 2026 by UTC: the same day by any clock from
    /// eleven hours behind Greenwich to eleven ahead.
    static let noon: Int64 = 1_783_857_600

    static let refusal = """
    {"error": {"code": "not_found", "message": "No thermal map here."}}
    """
}

/// What the server does next, one answer at a time, and what it was asked.
private actor ThermalServer {
    enum Answer {
        case body(Int, String)
        case unreachable
    }

    private var answers: [Answer]
    private(set) var asked: [URL] = []

    init(_ answers: [Answer]) {
        self.answers = answers
    }

    func answer(_ url: URL) throws -> (Int, Data) {
        asked.append(url)
        let next: Answer = answers.isEmpty ? .unreachable : answers.removeFirst()
        switch next {
        case .body(let status, let text):
            return (status, Data(text.utf8))
        case .unreachable:
            throw URLError(.notConnectedToInternet)
        }
    }
}

/// The forecast map's model against the real core, with the server played
/// by canned answers in the server's own format; and what the map tab
/// draws of it.
@MainActor
final class ThermalMapTests: XCTestCase {
    private func model(_ server: ThermalServer) throws -> ThermalMapModel {
        let model = ThermalMapModel(transport: { url in try await server.answer(url) })
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        model.server = { address }
        model.clock = { ServerThermals.noon }
        return model
    }

    func testTheMapIsAskedForAndReadByTheCore() async throws {
        let server = ThermalServer([.body(200, ServerThermals.answer)])
        let thermals = try model(server)
        XCTAssertEqual(thermals.state, .idle)

        await thermals.load()

        let asked = await server.asked
        XCTAssertEqual(asked.map(\.absoluteString), ["http://127.0.0.1:8080/v1/thermals"])
        XCTAssertEqual(thermals.state, .loaded)
        let map = try XCTUnwrap(thermals.map)
        // The day that is over is left out; the others are named by the core.
        XCTAssertEqual(map.days.map(\.name), ["Today", "Tomorrow"])
        XCTAssertEqual(map.model, "ICON-D2")
        XCTAssertEqual(map.legend.first?.scaleStep, 6)
        XCTAssertEqual(map.legend.last?.words, "Not flyable")
        XCTAssertNil(map.legend.last?.scaleStep)
        XCTAssertFalse(map.days[0].note.isEmpty)
        XCTAssertTrue(map.days[1].note.isEmpty)
        XCTAssertTrue(thermals.isFresh)
    }

    func testARefusalIsSaidInTheCoresWordsAndNothingIsDrawn() async throws {
        let server = ThermalServer([.body(404, ServerThermals.refusal)])
        let thermals = try model(server)

        await thermals.load()

        guard case .failed(let words) = thermals.state else {
            return XCTFail("A refusal is a failure, not \(thermals.state)")
        }
        XCTAssertFalse(words.isEmpty)
        XCTAssertNil(thermals.map)
        XCTAssertFalse(thermals.isFresh)
    }

    func testVentiOutOfReachIsSaid() async throws {
        let thermals = try model(ThermalServer([.unreachable]))

        await thermals.load()

        guard case .failed(let words) = thermals.state else {
            return XCTFail("No answer is a failure, not \(thermals.state)")
        }
        XCTAssertTrue(words.hasPrefix("Venti could not be reached for the forecast map."))
    }

    func testAMapFetchedEarlierStaysWhenVentiIsOutOfReach() async throws {
        let thermals = try model(ThermalServer([.body(200, ServerThermals.answer), .unreachable]))
        var clock = Date(timeIntervalSince1970: TimeInterval(ServerThermals.noon))
        thermals.now = { clock }

        await thermals.load()
        let first = try XCTUnwrap(thermals.map)
        XCTAssertTrue(thermals.isFresh)

        // Half an hour later the map is old, and is asked for again.
        clock += ThermalMapModel.keepFor
        XCTAssertFalse(thermals.isFresh)
        await thermals.load()

        XCTAssertEqual(thermals.state, .loaded)
        XCTAssertEqual(thermals.map, first)
    }

    func testVentiIsAskedAgainHalfAnHourAfterAMapAndFiveMinutesAfterAFailure() async throws {
        let thermals = try model(ThermalServer([.unreachable, .body(200, ServerThermals.answer)]))
        var clock = Date(timeIntervalSince1970: TimeInterval(ServerThermals.noon))
        thermals.now = { clock }
        XCTAssertTrue(thermals.isDue)

        await thermals.load()
        XCTAssertFalse(thermals.isDue)
        clock += ThermalMapModel.retryAfter
        XCTAssertTrue(thermals.isDue)

        await thermals.load()
        XCTAssertEqual(thermals.state, .loaded)
        clock += ThermalMapModel.retryAfter
        XCTAssertFalse(thermals.isDue)
        clock += ThermalMapModel.keepFor
        XCTAssertTrue(thermals.isDue)
    }

    func testAnotherServersMapIsNotKept() async throws {
        let thermals = try model(ThermalServer([.body(200, ServerThermals.answer), .unreachable]))
        await thermals.load()
        XCTAssertNotNil(thermals.map)

        let other = try XCTUnwrap(URL(string: "http://127.0.0.1:9090"))
        thermals.server = { other }
        XCTAssertFalse(thermals.isFresh)
        await thermals.load()

        XCTAssertNil(thermals.map)
        guard case .failed = thermals.state else {
            return XCTFail("The other server was not reached: \(thermals.state)")
        }
    }

    func testWithoutVentiThereIsNothingToAsk() async {
        let thermals = ThermalMapModel(transport: { _ in throw URLError(.badURL) })
        await thermals.load()
        XCTAssertEqual(thermals.state, .idle)
        XCTAssertNil(thermals.map)
    }

    func testASquareIsFilledInTheStepTheCoreNamesAndNotWhereThereIsNoEstimate() async throws {
        let thermals = try model(ThermalServer([.body(200, ServerThermals.answer)]))
        await thermals.load()
        let map = try XCTUnwrap(thermals.map)
        let today = map.days[0]
        // Three squares have a verdict today; the one without an
        // estimate of its thermals is not drawn.
        XCTAssertEqual(today.cells.count, 3)
        XCTAssertEqual(today.cells.map(\.status), ["Flyable", "Not flyable", "Marginal"])
        // What speaks against a day, in the core's words; nothing on a
        // flyable one, nor on a day the server gave no reasons for.
        XCTAssertEqual(today.cells.map(\.why), ["", "Rain", "Gusts"])
        XCTAssertTrue(map.days[1].cells.allSatisfy { $0.why.isEmpty })
        XCTAssertEqual(ThermalSquares.fill(of: today.cells[0]), 5)
        XCTAssertEqual(ThermalSquares.fill(of: today.cells[1]), 0)
        XCTAssertNil(ThermalSquares.fill(of: today.cells[2]))

        let squares = ThermalSquares.squares(of: today)
        XCTAssertEqual(squares.map(\.index), [0, 1])
        XCTAssertEqual(squares.map(\.fill), [5, 0])
        // A closed ring around the square, from its south-west corner.
        let ring = squares[0].ring
        XCTAssertEqual(ring.count, 5)
        XCTAssertEqual(ring.first, ring.last)
        XCTAssertEqual(ring[0].lat, 46.95, accuracy: 1e-9)
        XCTAssertEqual(ring[0].lon, 10.95, accuracy: 1e-9)
        XCTAssertEqual(ring[2].lat, 47.05, accuracy: 1e-9)
        XCTAssertEqual(ring[2].lon, 11.05, accuracy: 1e-9)

        // The map's layer reads the fill and finds the square again.
        let drawn = ThermalMapLayers.feature(for: squares[1])
        XCTAssertEqual(drawn.attribute(forKey: "id") as? String, "1")
        XCTAssertEqual(drawn.attribute(forKey: "fill") as? Int, 0)

        // Tomorrow every square has a kind of day: weak thermals in the
        // scale's second step, a big day in its sixth.
        XCTAssertEqual(ThermalSquares.squares(of: map.days[1]).map(\.fill), [6, 3, 2, 6])
        // Its words are the core's.
        let big = map.days[1].cells[0]
        XCTAssertEqual(big.thermalWords, "Big day")
        XCTAssertEqual(big.thermalLine, "2.2 m/s to a cloud base of 3400 m")
        XCTAssertEqual(big.groundM, 612)
    }

    func testTheChosenDayIsShownAndTheFirstWhenItIsGone() async throws {
        let thermals = try model(ThermalServer([.body(200, ServerThermals.answer)]))
        await thermals.load()
        let map = try XCTUnwrap(thermals.map)
        XCTAssertEqual(ThermalSquares.day("2026-07-13", in: map)?.name, "Tomorrow")
        XCTAssertEqual(ThermalSquares.day("", in: map)?.name, "Today")
        XCTAssertEqual(ThermalSquares.day("2026-07-11", in: map)?.name, "Today")
    }

    func testTheCardKnowsWhetherASquareLiesInView() async throws {
        let thermals = try model(ThermalServer([.body(200, ServerThermals.answer)]))
        await thermals.load()
        let today = try XCTUnwrap(thermals.map?.days.first)
        let around = Bounds(south: 46.9, west: 10.9, north: 47.1, east: 11.3)
        XCTAssertTrue(ThermalSquares.anyDrawn(of: today, in: around))
        // Only the square without an estimate lies here: nothing is drawn.
        let undrawn = Bounds(south: 46.9, west: 11.26, north: 47.1, east: 11.4)
        XCTAssertFalse(ThermalSquares.anyDrawn(of: today, in: undrawn))
        let elsewhere = Bounds(south: 40, west: -4, north: 41, east: -3)
        XCTAssertFalse(ThermalSquares.anyDrawn(of: today, in: elsewhere))
    }

    func testAStepOfTheScaleHasTheColourOfThePacksStep() {
        XCTAssertEqual(ThermalBand(scaleStep: 1).step, 0)
        XCTAssertEqual(ThermalBand(scaleStep: 6).step, 5)
        // Out of the scale's range, the nearest step.
        XCTAssertEqual(ThermalBand(scaleStep: 0).step, 0)
        XCTAssertEqual(ThermalBand(scaleStep: 9).step, 5)
    }
}
