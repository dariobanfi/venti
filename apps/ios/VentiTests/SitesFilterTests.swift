import MapLibre
import VentiCore
import XCTest

@testable import Venti

/// What a Venti server answers when asked for flying sites: a take-off
/// with its wind and its height, one that says nothing, a landing, and a
/// landing field mapped as an area, as the first region has it.
private enum FilterSites {
    static let answer = """
    {"layers": [{
        "source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z", "as_of": "2026-11-18T21:21:02Z",
        "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
        "sites": [
            {"id": "osm-free-flying:node/1", "name": "Wallberg Süd", "position": {"lat": 47.6652, "lon": 11.7954},
             "elevation_m": 1620.0, "uses": ["takeoff"],
             "wind_directions": [{"from_deg": 180.0, "to_deg": 225.0}]},
            {"id": "osm-free-flying:node/2", "position": {"lat": 47.6654, "lon": 11.7953}, "uses": ["takeoff"]},
            {"id": "osm-free-flying:node/3", "position": {"lat": 47.6919, "lon": 11.7880}, "uses": ["landing"]},
            {"id": "osm-free-flying:way/1080988126", "name": "Landeplatz Jägerkamp/Nagelspitz",
             "position": {"lat": 47.6924086, "lon": 11.8865478}, "uses": ["landing"],
             "outline": [[11.8880464, 47.6919322], [11.8875034, 47.6925871], [11.8879519, 47.692816],
                         [11.8874492, 47.6933534], [11.8858179, 47.6933335], [11.8850491, 47.6925639],
                         [11.8859455, 47.6914638], [11.8880464, 47.6919322]]}
        ]
    }],
    "attributions": [{
        "source": "osm-free-flying", "provider": "OpenStreetMap contributors", "licence": "ODbL 1.0",
        "text": "© OpenStreetMap contributors", "url": "https://www.openstreetmap.org/copyright"
    }]}
    """
}

/// The filter of the sites: the app's choice handed to the real core, and
/// what the map and the list are given back.
@MainActor
final class SitesFilterTests: XCTestCase {
    private let tegernsee = Bounds(south: 47.60, west: 11.65, north: 47.75, east: 11.90)

    /// A model that holds the four sites, with the filter kept in `store`.
    private func model(store: UserDefaults? = nil) async throws -> SitesModel {
        let sites = SitesModel(transport: { _ in (200, Data(FilterSites.answer.utf8)) }, store: store)
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        sites.server = { server }
        await sites.load(tegernsee, from: server)
        XCTAssertNil(sites.problem)
        return sites
    }

    func testAWindLeavesWhatFitsAndShowsApartWhatSaysNothing() async throws {
        let sites = try await model()
        // Nothing asked: every site, and nothing apart.
        XCTAssertFalse(sites.filtered.isOn)
        XCTAssertEqual(sites.filtered.fitting.count, 4)
        XCTAssertTrue(sites.filtered.unsaid.isEmpty)
        XCTAssertEqual(sites.filtered.filterWords, "All sites")
        XCTAssertEqual(sites.filtered.words, "4 sites")

        // A wind from the south: the take-off that names it fits, the one
        // that names none is apart, and the landings stay as they are.
        var choice = SitesChoice()
        choice.wind = .from(180)
        sites.choose(choice)
        XCTAssertTrue(sites.filtered.isOn)
        XCTAssertEqual(
            sites.filtered.fitting.map(\.name), ["Wallberg Süd", "Landing", "Landeplatz Jägerkamp/Nagelspitz"]
        )
        XCTAssertEqual(sites.filtered.unsaid.map(\.name), ["Take-off"])
        XCTAssertEqual(sites.filtered.words, "1 of 2 take-offs fits a wind from S")
        XCTAssertEqual(sites.filtered.unsaidTitle, "1 take-off gives no wind directions")
        XCTAssertFalse(sites.filtered.unsaidNote.isEmpty)
        XCTAssertEqual(sites.filtered.filterWords, "All sites, take-offs for a wind from S")
        XCTAssertEqual(
            SitesFilterLine.leaves(sites.filtered),
            "1 of 2 take-offs fits a wind from S\n1 take-off gives no wind directions"
        )

        // A wind from the north: the take-off that names the south is not
        // drawn; the one that says nothing stays, apart.
        choice.wind = .from(0)
        sites.choose(choice)
        XCTAssertEqual(sites.filtered.fitting.map(\.name), ["Landing", "Landeplatz Jägerkamp/Nagelspitz"])
        XCTAssertEqual(sites.filtered.unsaid.map(\.name), ["Take-off"])
        XCTAssertEqual(sites.filtered.words, "0 of 2 take-offs fit a wind from N")

        // Take-offs from 1500 m, in a south wind: both are asked.
        choice.shown = .takeoffs
        choice.wind = .from(180)
        choice.fromM = 1500
        sites.choose(choice)
        XCTAssertEqual(sites.filtered.fitting.map(\.name), ["Wallberg Süd"])
        XCTAssertEqual(sites.filtered.filterWords, "Take-offs for a wind from S, from 1500 m up")
        XCTAssertEqual(sites.filtered.unsaidTitle, "1 take-off gives no wind directions or no height")

        // Landings: the wind and the height are not asked, nor offered.
        choice.shown = .landings
        sites.choose(choice)
        XCTAssertEqual(sites.filtered.fitting.map(\.name), ["Landing", "Landeplatz Jägerkamp/Nagelspitz"])
        XCTAssertTrue(sites.filtered.unsaid.isEmpty)
        XCTAssertEqual(sites.filtered.filterWords, "Landings")
        XCTAssertFalse(SitesChoices.asksTakeoffs(.landings))
        XCTAssertTrue(SitesChoices.asksTakeoffs(.all))
    }

    func testTheChoicesAreTheCoresAndForecastNowIsNotOffered() {
        let core = SitesChoices.core
        XCTAssertEqual(core.shown.map(\.words), ["All sites", "Take-offs", "Landings"])
        XCTAssertEqual(core.heights.map(\.words), ["Any", "1000 m", "1500 m", "2000 m"])
        // Without a station's wind: any, and the sixteen points.
        let without = SitesChoices.winds(measuredFromDeg: nil)
        XCTAssertEqual(without.count, 17)
        XCTAssertEqual(without.prefix(3).map(\.name), ["Any", "N", "NNE"])
        XCTAssertEqual(without.last?.name, "NNW")
        // With one: "Measured now" after "Any". The app has no forecast
        // at hand for the map's middle, so "Forecast now" is left out.
        let with = SitesChoices.winds(measuredFromDeg: 200)
        XCTAssertEqual(with.prefix(3).map(\.name), ["Any", "Measured now", "N"])
        XCTAssertFalse(with.contains { $0.name == "Forecast now" })
    }

    func testTheFilterIsKeptAsThePilotLeftIt() async throws {
        let name = "venti-sites-filter-\(UUID().uuidString)"
        let store = try XCTUnwrap(UserDefaults(suiteName: name))
        defer { store.removePersistentDomain(forName: name) }

        let first = try await model(store: store)
        XCTAssertEqual(first.choice, SitesChoice())
        var choice = SitesChoice()
        choice.shown = .takeoffs
        choice.wind = .from(292.5)
        choice.fromM = 1500
        first.choose(choice)

        // The next launch: the same choice, and the same sites left.
        let second = try await model(store: store)
        XCTAssertEqual(second.choice, choice)
        XCTAssertEqual(second.filtered.filterWords, "Take-offs for a wind from WNW, from 1500 m up")

        // "Measured now" is kept as that, not as the direction it had.
        choice.wind = .measuredNow
        second.choose(choice)
        XCTAssertEqual(SitesChoice(keptIn: store).wind, .measuredNow)
        // And a filter that asks nothing again.
        second.choose(SitesChoice())
        XCTAssertEqual(SitesChoice(keptIn: store), SitesChoice())
    }

    func testMeasuredNowIsTheNearestStationsCurrentWind() async throws {
        func station(_ id: String, lat: Double, from: Double?, symbol: WindSymbol) -> WindStation {
            WindStation(
                id: id, name: id, source: "test", position: Coordinate(lat: lat, lon: 11.80),
                elevationM: 1_700,
                latest: WindReading(
                    timeUnixS: 1_000, speedMs: 4, gustMs: nil, directionDeg: from, temperatureC: nil,
                    humidityPct: nil
                ),
                ageMin: 10, isOld: symbol == .old,
                mark: WindMark(symbol: symbol, headingDeg: 0, label: "14")
            )
        }
        let middle = Coordinate(lat: 47.66, lon: 11.80)
        let stations = [
            station("far", lat: 47.90, from: 90, symbol: .calm),
            // The nearest reads no direction, the next is old: neither
            // is the wind now.
            station("no vane", lat: 47.661, from: nil, symbol: .calm),
            station("old", lat: 47.67, from: 10, symbol: .old),
            station("near", lat: 47.70, from: 185, symbol: .calm),
        ]
        XCTAssertEqual(SitesModel.measuredNow(stations: stations, around: middle), 185)
        XCTAssertNil(SitesModel.measuredNow(stations: [], around: middle))

        // Chosen without a direction it asks about no wind; with one, the
        // core says whose wind it is.
        let sites = try await model()
        var choice = SitesChoice()
        choice.wind = .measuredNow
        sites.choose(choice)
        XCTAssertFalse(sites.filtered.isOn)
        sites.measured(fromDeg: SitesModel.measuredNow(stations: stations, around: middle))
        XCTAssertTrue(sites.filtered.isOn)
        XCTAssertEqual(sites.filtered.words, "1 of 2 take-offs fits the S wind measured now")
        // The wind turns north: the map follows without being asked.
        sites.measured(fromDeg: 0)
        XCTAssertEqual(sites.filtered.words, "0 of 2 take-offs fit the N wind measured now")
    }

    func testWhatSaysNothingIsDrawnMutedAndWithoutItsName() async throws {
        let sites = try await model()
        let named = try XCTUnwrap(sites.sites.first)
        let fits = OverviewMap.Coordinator.siteFeature(for: named)
        XCTAssertEqual(fits.attribute(forKey: "picture") as? String, "site-takeoff")
        XCTAssertEqual(fits.attribute(forKey: "label") as? String, "Wallberg Süd")
        let apart = OverviewMap.Coordinator.siteFeature(for: named, isUnsaid: true)
        XCTAssertEqual(apart.attribute(forKey: "picture") as? String, "site-takeoff-unsaid")
        XCTAssertEqual(apart.attribute(forKey: "label") as? String, "")
        // The ones that fit lie over the ones shown apart.
        XCTAssertLessThan(
            try XCTUnwrap(apart.attribute(forKey: "rank") as? Int),
            try XCTUnwrap(fits.attribute(forKey: "rank") as? Int)
        )
        // Every kind has both pins, and the muted one is another picture.
        let pictures = SiteSymbols.all
        XCTAssertEqual(pictures.count, 2 * SiteKind.all.count)
        XCTAssertTrue(pictures.contains { $0.name == "site-takeoff-unsaid" })
        XCTAssertNotEqual(
            SiteSymbols.picture(for: .takeoff).pngData(), SiteSymbols.picture(for: .takeoff, isUnsaid: true).pngData()
        )
    }

    func testALandingFieldIsDrawnAsItsAreaWhereItIsLargerThanItsSign() async throws {
        let sites = try await model()
        let field = try XCTUnwrap(sites.sites.first { $0.id == "osm-free-flying:way/1080988126" })
        XCTAssertEqual(field.outline.count, 8)
        XCTAssertEqual(field.outline.first, field.outline.last)

        // The field is some 225 m long: larger than a sign of 28 points
        // on a map with 5 m to a point, a speck with 50 m to a point.
        let extent = SiteFields.extentM(of: field.outline)
        XCTAssertEqual(extent, 225, accuracy: 10)
        XCTAssertTrue(SiteFields.isLarger(field.outline, than: 28, metresPerPoint: 5))
        XCTAssertFalse(SiteFields.isLarger(field.outline, than: 28, metresPerPoint: 50))
        // A site mapped as a point has no area at any scale.
        XCTAssertFalse(SiteFields.isLarger([], than: 28, metresPerPoint: 0.1))

        // Close in, the one field among the four sites; far out, none.
        let close = SiteFields.fields(fitting: sites.sites, unsaid: [], metresPerPoint: 2, signPoints: 28)
        XCTAssertEqual(close.map(\.id), ["osm-free-flying:way/1080988126"])
        XCTAssertEqual(close.first?.isUnsaid, false)
        XCTAssertTrue(SiteFields.fields(fitting: sites.sites, unsaid: [], metresPerPoint: 80, signPoints: 28).isEmpty)
        // The field of a site shown apart is drawn as such.
        let apart = SiteFields.fields(fitting: [], unsaid: [field], metresPerPoint: 2, signPoints: 28)
        XCTAssertEqual(apart.first?.isUnsaid, true)

        // As the map reads it: a polygon of the ring's eight points.
        let drawn = SiteFields.feature(for: try XCTUnwrap(close.first))
        XCTAssertEqual(drawn.pointCount, 8)
        XCTAssertEqual(drawn.attribute(forKey: "unsaid") as? Int, 0)
        XCTAssertEqual(drawn.coordinates.pointee.latitude, 47.6919322, accuracy: 1e-6)
    }
}
