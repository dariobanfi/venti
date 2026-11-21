import VentiCore
import XCTest

@testable import Venti

/// Finding a pilot by name, the ranking of a month and a pilot's totals
/// on the iPhone: asked through the real core of a server played by
/// canned answers, which are the server's own golden ones.
@MainActor
final class PilotsTests: XCTestCase {
    private actor Asked {
        var addresses: [String] = []
        func add(_ url: URL) { addresses.append(url.absoluteString) }
    }

    private let address = URL(string: "http://127.0.0.1:8080")

    /// A server that finds Lena for "le" and nobody for anything else.
    private func community(noting asked: Asked) -> CommunityModel {
        let community = CommunityModel { url in
            await asked.add(url)
            let address = url.absoluteString
            if address.contains("/v1/pilots?") {
                return (200, Data((address.hasSuffix("q=le") ? PilotsAnswers.found : PilotsAnswers.nobody).utf8))
            }
            if address.contains("/statistics") {
                return address.contains("nobody")
                    ? (404, Data(#"{"error": {"code": "pilot_not_found", "message": "x"}}"#.utf8))
                    : (200, Data(PilotsAnswers.statistics.utf8))
            }
            if address.contains("/v1/ranking") {
                return (200, Data(PilotsAnswers.ranking.utf8))
            }
            return (404, Data())
        }
        let address = address
        community.server = { address }
        return community
    }

    func testAPilotIsFoundByNameOnceTheTypingRests() async throws {
        let asked = Asked()
        let community = community(noting: asked)
        let search = PilotSearch()

        // One letter is not asked for: the core's words stand in place
        // of pilots.
        search.search("l", in: community, after: .zero)
        XCTAssertNil(search.waiting)
        XCTAssertNil(search.found)
        XCTAssertEqual(search.words, "Type at least two letters of a pilot's name or handle.")
        XCTAssertFalse(search.isSearching)

        // Typing on before the rest is over asks once, for the last text.
        search.search("le", in: community, after: .seconds(30))
        XCTAssertTrue(search.isSearching)
        XCTAssertNil(search.words)
        search.search("len", in: community, after: .seconds(30))
        search.search("le ", in: community, after: .zero)
        await search.waiting?.value
        var addresses = await asked.addresses
        XCTAssertEqual(addresses, ["http://127.0.0.1:8080/v1/pilots?q=le"])
        XCTAssertFalse(search.isSearching)
        XCTAssertEqual(search.found?.pilots.map(\.words), ["Lena Probe (@lena), 3 public flights"])
        XCTAssertEqual(search.found?.pilots.first?.handle, "lena")
        XCTAssertEqual(search.found?.words, "1 pilot")

        // The same text again asks nothing.
        search.search("le", in: community, after: .zero)
        await search.waiting?.value
        addresses = await asked.addresses
        XCTAssertEqual(addresses.count, 1)

        // Nobody by a name: the core's line.
        search.search("xy", in: community, after: .zero)
        XCTAssertNil(search.found)
        await search.waiting?.value
        XCTAssertEqual(search.found?.pilots, [])
        XCTAssertEqual(search.found?.words, "No pilot by this name has shared a flight with everyone.")

        // The field emptied: nothing is shown and nothing asked.
        search.search("", in: community, after: .zero)
        XCTAssertNil(search.found)
        XCTAssertNil(search.words)
        addresses = await asked.addresses
        XCTAssertEqual(addresses.count, 2)
    }

    func testTheRankingIsOfAYearOrOneOfItsMonths() async throws {
        let asked = Asked()
        let community = community(noting: asked)
        let now = Date(timeIntervalSince1970: 1_795_000_000) // 18 November 2026

        // The newest season first, by the clock: the whole year.
        await community.rank(now: now)
        var last = await asked.addresses.last
        XCTAssertEqual(last, "http://127.0.0.1:8080/v1/ranking?from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z")
        XCTAssertEqual(community.ranked.title, "2026")
        XCTAssertNil(community.ranked.month)
        // The years offered are those the server says have a ranking,
        // and all years; the months those of the year shown.
        XCTAssertEqual(community.ranked.seasons.map(\.name), ["2026", "2025", "All years"])
        XCTAssertEqual(
            community.ranked.months.map(\.name), ["September", "August", "July", "June", "May", "April"]
        )

        // A month of it, asked for as the core writes it.
        await community.rank(month: "2026-05", now: now)
        last = await asked.addresses.last
        XCTAssertEqual(last, "http://127.0.0.1:8080/v1/ranking?month=2026-05")
        XCTAssertEqual(community.ranked.season, "2026")
        XCTAssertEqual(community.ranked.month, "2026-05")
        XCTAssertEqual(community.ranked.title, "May 2026")

        // Pulled down: the same month again.
        await community.rank(now: now)
        last = await asked.addresses.last
        XCTAssertEqual(last, "http://127.0.0.1:8080/v1/ranking?month=2026-05")

        // Another year is the whole of it, with its own months.
        await community.rank(season: "2025", now: now)
        last = await asked.addresses.last
        XCTAssertEqual(last, "http://127.0.0.1:8080/v1/ranking?from=2025-01-01T00:00:00Z&to=2026-01-01T00:00:00Z")
        XCTAssertNil(community.ranked.month)
        XCTAssertEqual(community.ranked.title, "2025")
        XCTAssertEqual(community.ranked.months.map(\.title), ["July 2025"])

        // All years have no months.
        await community.rank(season: "All years", now: now)
        last = await asked.addresses.last
        XCTAssertEqual(last, "http://127.0.0.1:8080/v1/ranking")
        XCTAssertEqual(community.ranked.title, "All years")
        XCTAssertTrue(community.ranked.months.isEmpty)

        // The whole year again from a month.
        await community.rank(season: "2026", now: now)
        await community.rank(month: "2026-08", now: now)
        await community.rank(month: nil, now: now)
        XCTAssertNil(community.ranked.month)
        XCTAssertEqual(community.ranked.title, "2026")
    }

    func testTheSeasonsOfferedAreTheServersYears() throws {
        let byTheClock = rankingSeasons(nowUnixS: 1_795_000_000)
        let shown = try XCTUnwrap(byTheClock.first)
        // A server that names no years: the core's, by the clock.
        XCTAssertEqual(CommunityModel.seasons(having: [], shown: shown, byTheClock: byTheClock), byTheClock)
        // The season shown stays offered where nothing was flown in it.
        let before = RankingYearChoice(
            name: "2019", fromUnixS: 1_546_300_800, toUnixS: 1_577_836_800, flights: 1, words: "", months: []
        )
        XCTAssertEqual(
            CommunityModel.seasons(having: [before], shown: shown, byTheClock: byTheClock).map(\.name),
            ["2026", "2019", "All years"]
        )
    }

    func testAPilotsTotalsAreReadSeasonBySeason() async throws {
        let asked = Asked()
        let community = community(noting: asked)

        let statistics = try await community.statistics(of: "lena")
        let last = await asked.addresses.last
        XCTAssertEqual(last, "http://127.0.0.1:8080/v1/pilots/lena/statistics")
        XCTAssertEqual(statistics.name, "Lena Probe")
        XCTAssertEqual(statistics.seasons.map(\.name), ["2026"])

        // All years where none is chosen, and for a year there is none of.
        let overall = PilotTotalsSections.shown(of: statistics, year: 0)
        XCTAssertEqual(overall.name, "All years")
        XCTAssertEqual(PilotTotalsSections.shown(of: statistics, year: 1999), overall)
        XCTAssertEqual(PilotTotalsSections.shown(of: statistics, year: 2026).name, "2026")

        let points = try XCTUnwrap(overall.rows.first { $0.label == "Points, best 3 flights" })
        XCTAssertEqual(points.value, "28.19")
        XCTAssertEqual(points.flightId, "")
        // The longest flight is a flight: its row opens it.
        XCTAssertEqual(overall.rows.first { $0.label == "Longest flight" }?.flightId, "ww5qkxhxaedefvypb3wy3v6m3m")
        XCTAssertEqual(overall.sites.map(\.words), ["Musterkogel West, 2 flights"])
        XCTAssertEqual(overall.months.count, 12)
        XCTAssertFalse(statistics.note.isEmpty)

        // A pilot Venti does not know is said in words, and what can be
        // no handle is not asked for.
        do {
            _ = try await community.statistics(of: "nobody")
            XCTFail("A pilot the server does not know had totals.")
        } catch {
            XCTAssertTrue(error.words.contains("no pilot"))
        }
        let before = await asked.addresses.count
        do {
            _ = try await community.statistics(of: "../accounts")
            XCTFail("A path was taken for a handle.")
        } catch {
            let after = await asked.addresses.count
            XCTAssertEqual(after, before)
        }
    }
}

/// What a server says: the answers of `crates/server/tests/golden`, the
/// ranking without its pilots.
private enum PilotsAnswers {
    static let terms = #"{"text": "These flights are their pilots'.", "decided": false}"#

    static let found = #"{"pilots": [{"pilot": {"handle": "lena", "display_name": "Lena Probe"}, "flights": 3}]}"#

    static let nobody = #"{"pilots": []}"#

    static let ranking = """
        {"counted": 3, "pilots": [], "periods": [
          {"year": 2026, "flights": 10, "months": [
            {"month": "2026-09", "flights": 1}, {"month": "2026-08", "flights": 4},
            {"month": "2026-07", "flights": 1}, {"month": "2026-06", "flights": 2},
            {"month": "2026-05", "flights": 1}, {"month": "2026-04", "flights": 1}]},
          {"year": 2025, "flights": 1, "months": [{"month": "2025-07", "flights": 1}]}],
         "terms": \(terms)}
        """

    private static let totals = """
        {"flights": 3, "scored": 3, "distance_m": 28191.0, "points": 28.19, "airtime_s": 3865,
         "longest": {"id": "ww5qkxhxaedefvypb3wy3v6m3m", "distance_m": 12007.4},
         "highest": {"id": "hwj4105bnzcgcavngacrzjt3s4", "max_altitude_m": 2117},
         "sites": [{"site": {"id": "venti:0000000000000000000000s001", "name": "Musterkogel West"}, "flights": 2}],
         "by_month": [0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0]}
        """

    static let statistics = """
        {"pilot": {"handle": "lena", "display_name": "Lena Probe"}, "counted": 3, "overall": \(totals),
         "seasons": [{"year": 2026, "totals": \(totals)}], "terms": \(terms)}
        """
}
