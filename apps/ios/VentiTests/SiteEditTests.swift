import VentiCore
import XCTest

@testable import Venti

/// Adding and correcting a site against the real core, with the server
/// played by answers in the server's own format.
@MainActor
final class SiteEditTests: XCTestCase {
    /// What was sent.
    private actor Sent {
        var requests: [ServerRequest] = []
        func add(_ request: ServerRequest) { requests.append(request) }
    }

    /// The server's answer about "Musterkogel West" at its third revision.
    private static let written = #"{"site": {"id": "venti:0000000000000000000000s001", "name": "Musterkogel West", "position": {"lat": 47.3, "lon": 10.9}, "elevation_m": 1871.0, "uses": ["takeoff"], "wind_directions": [{"from_deg": 225.0, "to_deg": 315.0}], "paragliding": true, "description": "A made-up site of the development mode. No such place exists."}, "standing": {"id": "venti:0000000000000000000000s001", "revision": 3, "created_at": "2026-10-29T19:05:00Z", "created_by": {"handle": "demo", "display_name": "Demo Pilot"}, "changed_at": "2026-11-18T19:05:00Z", "changed_by": {"handle": "petra", "display_name": "Pilot petra"}}, "lasting": true, "attributions": [{"source": "venti", "provider": "Venti contributors", "licence": "CC0 1.0", "text": "Venti contributors", "url": "https://creativecommons.org/publicdomain/zero/1.0/", "notice": "Added by pilots and not verified. A site on the map is not permission to fly there."}]}"#

    private static let tooClose = #"{"error": {"code": "site_too_close", "message": "x", "site": {"id": "venti:0000000000000000000000s001", "name": "Musterkogel West"}}}"#

    private func editing(
        answering status: UInt16, _ body: String, noting sent: Sent = Sent()
    ) -> SiteEditing {
        SiteEditing(
            server: URL(string: "http://venti.test"),
            send: { request in
                await sent.add(request)
                return AccountModel.Answer(status: status, body: Data(body.utf8), retryAfter: nil)
            },
            fetch: { _ in (Int(status), Data(body.utf8)) }
        )
    }

    /// A draft the core finds nothing wrong with.
    private func draft() -> SiteDraftView {
        var draft = siteDraftNew(place: Coordinate(lat: 47.295, lon: 10.895))
        draft.name = "Testkogel"
        draft.uses = [.takeoff]
        return draft
    }

    func testANewSiteIsSentSignedAndACorrectionToItsSite() async throws {
        let sent = Sent()
        let editing = editing(answering: 200, Self.written, noting: sent)

        let added = try await editing.write(draft(), comment: "")
        XCTAssertEqual(added.site.name, "Musterkogel West")
        XCTAssertEqual(added.site.byPilots?.revision, 3)
        XCTAssertEqual(added.words, "Saved for everyone, as revision 3 of this site.")

        // The form on the answer's site corrects that site, at its revision.
        var correction = try siteDraftOf(site: added.site)
        correction.setHeight("1880")
        _ = try await editing.write(correction, comment: "measured again")

        let requests = await sent.requests
        XCTAssertEqual(requests.map(\.method), ["POST", "PUT"])
        XCTAssertTrue(requests.allSatisfy(\.needsToken))
        XCTAssertTrue(requests[1].path.hasSuffix("0000000000000000000000s001"), requests[1].path)
        let body = String(decoding: requests[1].body ?? Data(), as: UTF8.self)
        XCTAssertTrue(body.contains("measured again"), body)
        XCTAssertTrue(body.contains("1880"), body)
    }

    func testARefusalForASiteInTheWayNamesTheSiteToOpen() async {
        let editing = editing(answering: 409, Self.tooClose)
        do {
            _ = try await editing.write(draft(), comment: "")
            XCTFail("The server refused, and nothing was thrown.")
        } catch let refused as SiteEditing.Refused {
            XCTAssertTrue(refused.words.contains("Musterkogel West"), refused.words)
            XCTAssertEqual(refused.inTheWay?.id, "venti:0000000000000000000000s001")
            XCTAssertEqual(refused.inTheWay?.words, "Open Musterkogel West")
            XCTAssertEqual(SiteEditing.words(for: refused), refused.words)
        } catch {
            XCTFail("\(error)")
        }
    }

    func testARefusalOfAnotherKindHasTheCoresWordsAndNoSite() async {
        let editing = editing(answering: 409, #"{"error": {"code": "site_changed", "message": "x"}}"#)
        do {
            _ = try await editing.write(draft(), comment: "")
            XCTFail("The server refused, and nothing was thrown.")
        } catch let refused as SiteEditing.Refused {
            XCTAssertTrue(refused.words.hasPrefix("Someone else changed this site"), refused.words)
            XCTAssertNil(refused.inTheWay)
        } catch {
            XCTFail("\(error)")
        }
    }

    func testASitesPageIsReadWithoutAnAccountAndRemovingSaysWhatBecameOfIt() async throws {
        let page = try await editing(answering: 200, Self.written).page("venti:0000000000000000000000s001")
        XCTAssertTrue(page.canBeCorrected)
        XCTAssertEqual(page.site.byPilots?.revision, 3)

        let sent = Sent()
        let words = try await editing(answering: 204, "", noting: sent)
            .remove("venti:0000000000000000000000s001", comment: "not a site")
        XCTAssertTrue(words.hasPrefix("Removed from the map."), words)
        let requests = await sent.requests
        XCTAssertEqual(requests.map(\.method), ["DELETE"])
    }

    func testAHeightIsReadFromItsFieldAndWhatIsNoNumberIsTheCoresToName() {
        var draft = draft()
        draft.setHeight(" 1862 ")
        XCTAssertEqual(draft.elevationM, 1862)
        XCTAssertEqual(draft.heightText, "1862")
        XCTAssertTrue(siteDraftCheck(draft: draft, comment: "").isEmpty)

        draft.setHeight("")
        XCTAssertNil(draft.elevationM)
        XCTAssertEqual(draft.heightText, "")
        XCTAssertTrue(siteDraftCheck(draft: draft, comment: "").isEmpty)

        // Not a number: the core says what a height is, at the height.
        draft.setHeight("high")
        XCTAssertEqual(siteDraftCheck(draft: draft, comment: "").map(\.field), [.height])

        // A site that is new and empty: the core names the name and the use.
        let empty = siteDraftNew(place: Coordinate(lat: 47.295, lon: 10.895))
        let fields = Set(siteDraftCheck(draft: empty, comment: "").map(\.field))
        XCTAssertTrue(fields.contains(.name), "\(fields)")
    }

    func testWhatThePilotEnteredIsCarriedOntoASiteSomeoneElseChanged() throws {
        let site = try sitePage(status: 200, body: Data(Self.written.utf8)).site
        let begun = try siteDraftOf(site: site)

        // The pilot corrects the height and ticks a wind direction.
        var mine = begun
        mine.setHeight("1880")
        mine.toggleWind(180)

        // Meanwhile someone else renamed the site, at revision 4.
        var now = begun
        now.name = "Musterkogel West ramp"
        now.revision = 4

        let carried = mine.carried(from: begun, onto: now)
        XCTAssertEqual(carried.revision, 4)
        XCTAssertEqual(carried.name, "Musterkogel West ramp")
        XCTAssertEqual(carried.elevationM, 1880)
        XCTAssertTrue(carried.windFromDeg.contains(180))
        XCTAssertEqual(carried.position, now.position)

        // Nothing entered: the site as it stands now.
        XCTAssertEqual(begun.carried(from: begun, onto: now), now)
    }
}
