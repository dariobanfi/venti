import VentiCore
import XCTest

@testable import Venti

/// What the development server answers at `/v1/foehn`, with the last four
/// points of each pair's 24 hours: Bolzano and Innsbruck at "likely",
/// Klagenfurt and Salzburg at "none", read at 19:05 UTC on 18 November 2026.
/// Every pressure is reduced to sea level by its provider, as in the
/// server's golden file `crates/server/tests/golden/foehn.json`.
private enum FoehnAnswer {
    static let body = #"""
    {"time":"2026-11-18T19:05:00Z","possible_from_hpa":3.0,"likely_from_hpa":6.0,"fresh_for_s":3600,"pairs":[
    {"id":"bolzano-innsbruck","south":{"id":"suedtirol-meteo:83200MS","source":"suedtirol-meteo","name":"Bolzano","position":{"lat":46.4977,"lon":11.3128},"elevation_m":245.0,"pressure":{"time":"2026-11-18T18:50:00Z","sea_level_hpa":1007.9,"reduced_by":"provider"}},"north":{"id":"geosphere-tawes:11320","source":"geosphere-tawes","name":"Innsbruck","position":{"lat":47.26,"lon":11.3842},"elevation_m":578.0,"pressure":{"time":"2026-11-18T19:00:00Z","sea_level_hpa":1001.7,"reduced_by":"provider"}},"level":"likely","difference_hpa":6.2,"difference_time":"2026-11-18T18:50:00Z","higher":"south","foehn":"south","series":[{"time":"2026-11-18T12:50:00Z","difference_hpa":5.0},{"time":"2026-11-18T14:50:00Z","difference_hpa":5.4},{"time":"2026-11-18T16:50:00Z","difference_hpa":5.8},{"time":"2026-11-18T18:50:00Z","difference_hpa":6.2}],"to_verify":false,"basis":"The pair and the limits of 3 and 6 hPa are those of the foehn chart of the weather service of the Province of Bolzano, which gives them for north foehn; for south foehn Venti uses the same limits, which no weather service has given."},
    {"id":"klagenfurt-salzburg","south":{"id":"geosphere-tawes:11331","source":"geosphere-tawes","name":"Klagenfurt","position":{"lat":46.6483,"lon":14.3183},"elevation_m":450.0,"pressure":{"time":"2026-11-18T18:50:00Z","sea_level_hpa":1001.0,"reduced_by":"provider"}},"north":{"id":"geosphere-tawes:11150","source":"geosphere-tawes","name":"Salzburg","position":{"lat":47.7894,"lon":13.0086},"elevation_m":430.0,"pressure":{"time":"2026-11-18T18:50:00Z","sea_level_hpa":1000.2,"reduced_by":"provider"}},"level":"none","difference_hpa":0.8,"difference_time":"2026-11-18T18:50:00Z","higher":"south","series":[{"time":"2026-11-18T12:50:00Z","difference_hpa":0.8},{"time":"2026-11-18T14:50:00Z","difference_hpa":0.8},{"time":"2026-11-18T16:50:00Z","difference_hpa":0.8},{"time":"2026-11-18T18:50:00Z","difference_hpa":0.8}],"to_verify":true,"basis":"Venti's own choice of two airfields on either side of the Tauern, with the limits of the pair Bolzano and Innsbruck. No weather service has given either."}],
    "caution":"A pressure difference across the main ridge is a sign of foehn, not a measurement of it. Foehn can blow with a small difference, and with a large one it can stay above a valley's cold air for hours and then break through within minutes. It says nothing about one valley or one take-off: read the wind stations, the forecast and the sky, and when in doubt do not fly.",
    "attributions":[{"source":"geosphere-tawes","provider":"GeoSphere Austria","short_name":"GeoSphere","licence":"CC BY 4.0","text":"Data source: GeoSphere Austria - https://data.hub.geosphere.at","url":"https://data.hub.geosphere.at"},{"source":"suedtirol-meteo","provider":"Autonome Provinz Bozen - Südtirol","short_name":"South Tyrol","licence":"CC0 1.0","text":"Data source: Autonome Provinz Bozen - Südtirol, Agentur für Bevölkerungsschutz","url":"https://data.civis.bz.it/de/dataset/misure-meteo-e-idrografiche"}]}
    """#

    /// The same answer with Innsbruck's pressure reduced to sea level by
    /// Venti itself, as when GeoSphere sends only the station's pressure.
    static let reducedByVenti: String = {
        let provider = #""sea_level_hpa":1001.7,"reduced_by":"provider""#
        precondition(body.contains(provider))
        return body.replacingOccurrences(
            of: provider, with: #""sea_level_hpa":1001.7,"reduced_by":"venti""#)
    }()

    /// A quarter of an hour after the newest reading.
    static let now: Int64 = 1_795_028_700
}

/// How often the server was asked.
private actor Asked {
    private(set) var count = 0

    func note() {
        count += 1
    }
}

/// Foehn on Today: the model over the core, against a recorded answer.
@MainActor
final class FoehnModelTests: XCTestCase {
    private let server = URL(string: "http://127.0.0.1:8080")
    private let tegernsee = Coordinate(lat: 47.6652, lon: 11.7954)

    func testTodayHasTheCoresLineForTakeoffsNearAPair() async throws {
        let asked = Asked()
        let model = FoehnModel(transport: { url in
            XCTAssertEqual(url.absoluteString, "http://127.0.0.1:8080/v1/foehn")
            await asked.note()
            return (200, Data(FoehnAnswer.body.utf8))
        })
        XCTAssertNil(model.line)
        XCTAssertEqual(model.title, "Foehn")

        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now })
        XCTAssertEqual(model.state, .loaded)
        let view = try XCTUnwrap(model.view)
        let line = try XCTUnwrap(model.line)
        XCTAssertEqual(line.level, .likely)
        XCTAssertEqual(line.pairId, "bolzano-innsbruck")
        XCTAssertTrue(line.text.contains("Bolzano"), line.text)
        XCTAssertEqual(model.title, view.title)

        // The view: both pairs, the first with its level, its age and its
        // points, the second with no sign and no status.
        XCTAssertEqual(view.rows.map(\.id), ["bolzano-innsbruck", "klagenfurt-salzburg"])
        XCTAssertEqual(view.rows.map(\.level), [.likely, FoehnLevel.none])
        XCTAssertEqual(view.rows[0].level.status, .danger)
        XCTAssertNil(view.rows[1].level.status)
        XCTAssertNil(FoehnLevel.unknown.status)
        XCTAssertEqual(FoehnLevel.possible.status, .caution)
        XCTAssertEqual(view.rows[0].points.count, 4)
        XCTAssertFalse(view.rows[0].when.isEmpty)
        XCTAssertEqual(view.credits.count, 2)

        // The chart reaches the same way up and down, past the limits and
        // the largest difference: zero is its middle.
        XCTAssertEqual(FoehnChart.reach(of: view.rows[0].points, limits: view.limits), 8)
        XCTAssertEqual(FoehnChart.reach(of: [], limits: []), 1)

        // Read again within minutes for other places: nothing is asked,
        // and far from every pair there is no line, though the view stays.
        await model.load(from: server, places: [Coordinate(lat: 52.5, lon: 13.4)], now: { FoehnAnswer.now })
        let once = await asked.count
        XCTAssertEqual(once, 1)
        XCTAssertNil(model.line)
        XCTAssertEqual(model.view?.rows.count, 2)

        // A pull on Today asks anew.
        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now }, anew: true)
        let twice = await asked.count
        XCTAssertEqual(twice, 2)
        XCTAssertNotNil(model.line)
    }

    /// A pressure that Venti reduced itself gives the view its row, with its
    /// level and a note that says so, and Today no line: a warning there
    /// rests only on the providers' own pressures.
    func testAPressureVentiReducedGivesTheViewItsRowAndTodayNoLine() async throws {
        let model = FoehnModel(transport: { _ in (200, Data(FoehnAnswer.reducedByVenti.utf8)) })
        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now })
        XCTAssertEqual(model.state, .loaded)
        XCTAssertNil(model.line)
        let view = try XCTUnwrap(model.view)
        XCTAssertEqual(view.rows.map(\.id), ["bolzano-innsbruck", "klagenfurt-salzburg"])
        let row = view.rows[0]
        XCTAssertEqual(row.level, .likely)
        XCTAssertTrue(row.words.contains("Bolzano"), row.words)
        XCTAssertTrue(row.note.contains("Innsbruck sent only"), row.note)
        XCTAssertTrue(row.note.hasSuffix("gives Today no line."), row.note)

        // Against an unknown clock too.
        await model.load(from: server, places: [tegernsee], now: { 0 }, anew: true)
        XCTAssertEqual(model.view?.rows.first?.level, .likely)
        XCTAssertNil(model.line)
    }

    /// Against an unknown clock a reading has no age: the core's words
    /// carry the reading's own time, and the pair is judged as it stands.
    func testAnUnknownClockGivesTheReadingsOwnTime() async throws {
        let model = FoehnModel(transport: { _ in (200, Data(FoehnAnswer.body.utf8)) })
        await model.load(from: server, places: [tegernsee], now: { 0 })
        let row = try XCTUnwrap(model.view?.rows.first)
        XCTAssertEqual(row.level, .likely)
        XCTAssertTrue(row.when.contains("UTC"), row.when)
        XCTAssertEqual(model.line?.level, .likely)
    }

    /// A reading older than an hour by Venti's clock has no level, and
    /// Today no line: old is never shown as now.
    func testOldReadingsGiveNoLine() async throws {
        let model = FoehnModel(transport: { _ in (200, Data(FoehnAnswer.body.utf8)) })
        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now + 3 * 3600 })
        XCTAssertEqual(model.state, .loaded)
        XCTAssertNil(model.line)
        XCTAssertEqual(model.view?.rows.first?.level, .unknown)
    }

    func testAVentiThatIsNotReachedSaysSoAndTodayHasNoLine() async throws {
        let answers = Asked()
        let model = FoehnModel(transport: { _ in
            await answers.note()
            let count = await answers.count
            if count == 1 {
                return (200, Data(FoehnAnswer.body.utf8))
            }
            if count == 2 {
                return (503, Data())
            }
            throw URLError(.notConnectedToInternet)
        })
        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now })
        XCTAssertNotNil(model.line)

        // Refused: the core's words, and what was read before is gone.
        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now }, anew: true)
        XCTAssertEqual(model.state, .failed("The server is busy or being restarted. Try again in a moment."))
        XCTAssertNil(model.view)
        XCTAssertNil(model.line)

        // Not reached.
        await model.load(from: server, places: [tegernsee], now: { FoehnAnswer.now })
        guard case .failed(let words) = model.state else {
            return XCTFail("\(model.state)")
        }
        XCTAssertTrue(words.hasPrefix("Venti could not be reached for foehn."), words)
        XCTAssertNil(model.line)

        // Without a server nothing is asked.
        await model.load(from: nil, places: [tegernsee], now: { FoehnAnswer.now })
        XCTAssertEqual(model.state, .failed("Venti could not be reached."))
        let asked = await answers.count
        XCTAssertEqual(asked, 3)
    }
}
