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
        XCTAssertEqual(row.score?.kind, .freeFlight)

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

    func testWhatWasFlownFromAPlaceIsAskedForAsItsSurroundings() async throws {
        let asked = Asked()
        let community = CommunityModel(transport: server(noting: asked, sample: try sampleText()))
        let address = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        community.server = { address }

        let flights = try await community.flights(near: Coordinate(lat: 47.6642, lon: 11.7967))
        XCTAssertEqual(flights.count, 2)
        let last = await asked.addresses.last
        let question = try XCTUnwrap(last?.absoluteString)
        // The square a kilometre and a half to each side of the place.
        XCTAssertTrue(question.hasPrefix("http://127.0.0.1:8080/v1/flights/public?bbox=11.77"), question)
        XCTAssertFalse(question.contains("sort="))
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
