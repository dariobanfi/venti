import VentiCore
import XCTest

@testable import Venti

/// Other pilots' flights on the iPhone: the list read through the real
/// core from a server played by canned answers, and a flight opened from
/// its file.
@MainActor
final class CommunityTests: XCTestCase {
    private actor Asked {
        var addresses: [URL] = []
        func add(_ url: URL) { addresses.append(url) }
    }

    /// A server with two pages of public flights and the sample flight as
    /// every flight's file.
    private func server(noting asked: Asked, sample: String) -> CommunityModel.Transport {
        { url in
            await asked.add(url)
            let address = url.absoluteString
            if address.contains("/v1/pilots/") {
                return address.contains("nobody")
                    ? (404, Data(#"{"error": {"code": "pilot_not_found", "message": "x"}}"#.utf8))
                    : (200, Data(CommunityAnswers.pilot(ids: ["aaaaaaaaaaaaaaaaaaaaaaaaaa"]).utf8))
            }
            if address.hasSuffix("/igc") {
                return address.contains("gone") ? (404, Data()) : (200, Data(sample.utf8))
            }
            if address.contains("after=second") {
                return (200, Data(CommunityAnswers.page(ids: ["cccccccccccccccccccccccccc"], next: nil).utf8))
            }
            return (
                200,
                Data(
                    CommunityAnswers.page(
                        ids: ["aaaaaaaaaaaaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbbbbbbbbbbbb"], next: "second"
                    ).utf8
                )
            )
        }
    }

    private func sampleText() throws -> String {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    func testThePublicFlightsAreListedPageByPage() async throws {
        let asked = Asked()
        let community = CommunityModel(transport: server(noting: asked, sample: try sampleText()))
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))

        // Without a server there is nothing to list and nothing is asked.
        await community.loadIfNeeded()
        XCTAssertTrue(community.flights.isEmpty)
        var count = await asked.addresses.count
        XCTAssertEqual(count, 0)

        community.server = { address }
        await community.loadIfNeeded()
        XCTAssertEqual(community.flights.map(\.id), ["aaaaaaaaaaaaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbbbbbbbbbbbbb"])
        XCTAssertTrue(community.hasMore)
        XCTAssertNil(community.problem)
        let first = await asked.addresses.first
        XCTAssertEqual(first?.absoluteString, "http://127.0.0.1:8080/v1/flights/public")

        // A row says who, when, on what and how far.
        let row = try XCTUnwrap(community.flights.first)
        XCTAssertEqual(row.pilot, "Mara Muster")
        XCTAssertTrue(PublicFlightRow.detail(of: row).hasSuffix("· Sample Wing"))
        // With the site the server says it took off from, between the two.
        var named = row
        named.takeoffName = "Wallberg"
        XCTAssertTrue(PublicFlightRow.detail(of: named).hasSuffix("· Wallberg · Sample Wing"))
        named.takeoffName = nil
        XCTAssertEqual(PublicFlightRow.detail(of: named), PublicFlightRow.detail(of: row))
        XCTAssertEqual(row.score?.kind, .freeFlight)
        // The take-off's flag before its name, the class before the glider,
        // and VoiceOver hears the country's name in place of the flag.
        named.takeoffName = "Wallberg"
        named.takeoffCountry = "AT"
        named.gliderClass = .b
        XCTAssertTrue(PublicFlightRow.detail(of: named).hasSuffix("· 🇦🇹 Wallberg · EN B · Sample Wing"))
        XCTAssertTrue(PublicFlightRow.spokenDetail(of: named).hasSuffix(", Wallberg, Austria, EN B · Sample Wing"))
        named.takeoffName = nil
        XCTAssertTrue(PublicFlightRow.detail(of: named).hasSuffix("· 🇦🇹 · EN B · Sample Wing"))

        // Asked for again, it is there already.
        await community.loadIfNeeded()
        count = await asked.addresses.count
        XCTAssertEqual(count, 1)

        // The last row in view fetches what follows it, once.
        await community.loadMore()
        XCTAssertEqual(community.flights.count, 3)
        XCTAssertFalse(community.hasMore)
        await community.loadMore()
        count = await asked.addresses.count
        XCTAssertEqual(count, 2)
        let second = await asked.addresses.last
        XCTAssertEqual(second?.absoluteString, "http://127.0.0.1:8080/v1/flights/public?after=second")

        // The best first: asked for anew, from the top.
        await community.show(.best)
        let best = await asked.addresses.last
        XCTAssertEqual(best?.absoluteString, "http://127.0.0.1:8080/v1/flights/public?sort=score")
        XCTAssertEqual(community.order, .best)
        XCTAssertEqual(community.flights.count, 2)
    }

    func testAnotherPilotsFlightOpensLikeOnesOwnAndNothingIsKept() async throws {
        let asked = Asked()
        let community = CommunityModel(transport: server(noting: asked, sample: try sampleText()))
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        community.server = { address }
        await community.loadIfNeeded()
        let flight = try XCTUnwrap(community.flights.first)

        let (view, analysis) = try await community.open(flight)
        let file = await asked.addresses.last
        XCTAssertEqual(
            file?.absoluteString, "http://127.0.0.1:8080/v1/flights/aaaaaaaaaaaaaaaaaaaaaaaaaa/igc"
        )
        XCTAssertEqual(view.id, "")
        XCTAssertGreaterThan(view.track.count, 1000)
        XCTAssertEqual(analysis?.thermals.count, 2)
    }

    func testAPilotsPublicFlightsAreListed() async throws {
        let asked = Asked()
        let community = CommunityModel(transport: server(noting: asked, sample: try sampleText()))
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        community.server = { address }

        let page = try await community.pilot("mara")
        let lookedUp = await asked.addresses.last
        XCTAssertEqual(lookedUp?.absoluteString, "http://127.0.0.1:8080/v1/pilots/mara")
        XCTAssertEqual(page.name, "Mara Muster")
        XCTAssertEqual(page.flights.map(\.id), ["aaaaaaaaaaaaaaaaaaaaaaaaaa"])
        XCTAssertNil(page.next)

        // A handle the server does not know is said in words, and what
        // can be no handle is not asked for at all.
        do {
            _ = try await community.pilot("nobody")
            XCTFail("A pilot the server does not know was listed.")
        } catch {
            XCTAssertTrue(error.words.contains("no pilot"))
        }
        let before = await asked.addresses.count
        do {
            _ = try await community.pilot("../accounts")
            XCTFail("A path was taken for a handle.")
        } catch {
            let after = await asked.addresses.count
            XCTAssertEqual(after, before)
        }
    }

    func testAFlightThatIsNoLongerPublicIsSaidSo() async throws {
        let asked = Asked()
        let community = CommunityModel(transport: server(noting: asked, sample: try sampleText()))
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        community.server = { address }
        await community.loadIfNeeded()
        var gone = try XCTUnwrap(community.flights.first)
        gone.filePath = "/v1/flights/gone/igc"
        do {
            _ = try await community.open(gone)
            XCTFail("A flight the server no longer has was opened.")
        } catch {
            XCTAssertTrue(CommunityModel.words(for: error).contains("no longer public"))
        }
    }

    /// A search taken apart into its chips in the core's words and order,
    /// counted for the filter's button, one choice let go of, and an area
    /// the core refuses in words before anything is asked.
    func testASearchIsTakenApartIntoTheCoresChips() {
        XCTAssertFalse(FlightSearch.everything.isSearching)
        XCTAssertEqual(FlightSearch.everything.count, 0)
        let search = FlightSearch(
            near: Coordinate(lat: 47.45, lon: 11.6), radiusKm: 40, gliderClass: .c, minKm: 50, country: "AT"
        )
        XCTAssertEqual(search.chips.map(\.words), ["Within 40 km", "EN C", "50 km or more", "AT"])
        XCTAssertEqual(search.chips.map(\.part), [.near, .gliderClass, .minKm, .country])
        XCTAssertEqual(search.count, 4)
        XCTAssertNil(search.refusal)
        let rest = search.without(.near)
        XCTAssertNil(rest.near)
        XCTAssertNil(rest.radiusKm)
        XCTAssertEqual(rest.count, 3)
        // A drawn area takes the place of the circle, and the other way.
        let corners = [
            Coordinate(lat: 47.7, lon: 11.7), Coordinate(lat: 47.7, lon: 11.9), Coordinate(lat: 47.6, lon: 11.8),
        ]
        let drawn = search.withArea(near: nil, radiusKm: nil, area: corners)
        XCTAssertEqual(drawn.chips.map(\.words).first, "Drawn area")
        XCTAssertNil(drawn.near)
        XCTAssertEqual(drawn.withArea(near: corners[0], radiusKm: 5, area: []).area, [])
        // Too wide a radius, or two corners, are refused in the core's words.
        XCTAssertEqual(
            FlightSearch(near: corners[0], radiusKm: 500).refusal, "Choose a radius from 1 to 100 km."
        )
        XCTAssertNotNil(FlightSearch(area: Array(corners.prefix(2))).refusal)
        // The least distance as typed.
        XCTAssertEqual(FlightSearchSheet.km("50"), 50)
        XCTAssertEqual(FlightSearchSheet.km("12,5"), 12.5)
        XCTAssertNil(FlightSearchSheet.km("0"))
        XCTAssertNil(FlightSearchSheet.km(""))
        XCTAssertEqual(FlightSearchSheet.km("5000"), FlightSearchChoices.bounds.maxMinKm)
        // A month of the ranking as its span by UTC.
        let may = CommunityModel.span(ofMonth: "2026-05")
        XCTAssertEqual(may?.fromUnixS, 1_777_593_600)
        XCTAssertEqual(may?.toUnixS, 1_780_272_000)
        XCTAssertNil(CommunityModel.span(ofMonth: "2026-13"))
    }

    /// The list asked again for a search, from its top, with the server's
    /// own names; the same search not asked twice; everything again the
    /// plain list; a pilot's flights searched the same way.
    func testTheListIsAskedAgainForASearch() async throws {
        let asked = Asked()
        let community = CommunityModel(transport: server(noting: asked, sample: try sampleText()))
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        community.server = { address }
        await community.loadIfNeeded()

        let search = FlightSearch(near: Coordinate(lat: 47.45, lon: 11.6), radiusKm: 40, gliderClass: .c)
        await community.find(search)
        XCTAssertEqual(community.search, search)
        let searchedAsked = await asked.addresses.last?.absoluteString
        let searched = try XCTUnwrap(searchedAsked)
        XCTAssertTrue(searched.hasPrefix("http://127.0.0.1:8080/v1/flights/public?"), searched)
        XCTAssertTrue(searched.contains("radius_km=40"), searched)
        XCTAssertTrue(searched.contains("class=en_c"), searched)
        XCTAssertEqual(community.flights.count, 2)
        // The next page keeps the search.
        await community.loadMore()
        let moreAsked = await asked.addresses.last?.absoluteString
        let more = try XCTUnwrap(moreAsked)
        XCTAssertTrue(more.contains("class=en_c") && more.contains("after=second"), more)

        let count = await asked.addresses.count
        await community.find(search)
        let again = await asked.addresses.count
        XCTAssertEqual(again, count)

        await community.find(.everything)
        let plain = await asked.addresses.last?.absoluteString
        XCTAssertEqual(plain, "http://127.0.0.1:8080/v1/flights/public")

        _ = try await community.pilot("mara", fromUnixS: nil, toUnixS: nil, search: FlightSearch(kind: .faiTriangle))
        let pilotAsked = await asked.addresses.last?.absoluteString
        let pilot = try XCTUnwrap(pilotAsked)
        XCTAssertTrue(pilot.hasPrefix("http://127.0.0.1:8080/v1/flights/public?"), pilot)
        XCTAssertTrue(pilot.contains("kind=fai_triangle") && pilot.hasSuffix("pilot=mara"), pilot)
    }

    /// A country's flag is drawn from its two letters and its name is
    /// `Locale`'s in English; the list to choose from has the countries
    /// and none of the continents.
    func testCountriesHaveFlagsAndNames() {
        XCTAssertEqual(Countries.flag("DE"), "🇩🇪")
        XCTAssertEqual(Countries.flag(" at "), "🇦🇹")
        XCTAssertNil(Countries.flag(nil))
        XCTAssertNil(Countries.flag("DEU"))
        XCTAssertNil(Countries.flag("1A"))
        XCTAssertEqual(Countries.name("de"), "Germany")
        XCTAssertEqual(Countries.named("CH"), "🇨🇭 Switzerland")
        XCTAssertTrue(Countries.all.contains { $0.code == "FR" && $0.name == "France" })
        XCTAssertFalse(Countries.all.contains { $0.code == "150" })
        XCTAssertTrue(Countries.all.count > 200)
        XCTAssertEqual(Countries.finding("germ").map(\.code), ["DE"])
        XCTAssertEqual(Countries.finding("it").first { $0.code == "IT" }?.name, "Italy")
    }
}

/// What a server says. Outside the test class, so that the stand-in for
/// the network may read it from wherever it runs.
private enum CommunityAnswers {
    static func pilot(ids: [String]) -> String {
        let flights = page(ids: ids, next: nil).dropFirst()
        return "{\"pilot\": {\"handle\": \"mara\", \"display_name\": \"Mara Muster\"}, \(flights)"
    }

    static func page(ids: [String], next: String?) -> String {
        let flights = ids.map { id in
            """
            {"id": "\(id)", "identity": "0f0d88442e01f52f291d25b2660b8bed",
             "pilot": {"handle": "mara", "display_name": "Mara Muster"},
             "visibility": "public", "uploaded_at": "2026-11-18T19:05:00Z",
             "start": "2026-08-15T11:10:00Z", "airtime_s": 1500,
             "takeoff": {"time": "2026-08-15T11:10:00Z", "position": {"lat": 47.6642, "lon": 11.7967},
                         "altitude_m": 1620},
             "track_length_m": 15877.9, "max_altitude_m": 2117, "max_climb_ms": 2.4,
             "score": {"kind": "free_flight", "closed": false, "distance_m": 9800.0, "points": 9.8},
             "glider": "Sample Wing", "outline": [[47.6642, 11.7967], [47.6697, 11.78986]],
             "file_bytes": 68008, "analysis_version": 1}
            """
        }
        let after = next.map { ", \"next\": \"\($0)\"" } ?? ""
        return "{\"flights\": [\(flights.joined(separator: ","))]\(after)}"
    }
}
