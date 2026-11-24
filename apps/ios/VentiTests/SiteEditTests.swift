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

    func testASiteSomeoneElseChangedIsToldApartAndTheCoreCarriesTheDraftOntoIt() async throws {
        let site = try sitePage(status: 200, body: Data(Self.written.utf8)).site
        // The pilot corrects the height and ticks a wind direction.
        var mine = try siteDraftOf(site: site)
        mine.setHeight("1880")
        mine.toggleWind(180)

        // The server refuses: someone else changed the site. The plumbing
        // says so with the core's reading of the answer, and a refusal of
        // another kind does not.
        do {
            _ = try await editing(answering: 409, #"{"error": {"code": "site_changed", "message": "x"}}"#)
                .write(mine, comment: "")
            XCTFail("The server refused, and nothing was thrown.")
        } catch let refused as SiteEditing.Refused {
            XCTAssertTrue(refused.wasChanged)
        }
        do {
            _ = try await editing(answering: 409, Self.tooClose).write(mine, comment: "")
            XCTFail("The server refused, and nothing was thrown.")
        } catch let refused as SiteEditing.Refused {
            XCTAssertFalse(refused.wasChanged)
        }

        // Meanwhile someone else renamed the site, at revision 4: the
        // form reads it as it stands and hands the three to the core.
        let renamed = Self.written
            .replacingOccurrences(of: #""name": "Musterkogel West""#, with: #""name": "Musterkogel West ramp""#)
            .replacingOccurrences(of: #""revision": 3"#, with: #""revision": 4"#)
        let now = try await editing(answering: 200, renamed).page(site.id).site
        let rebased = try siteDraftRebased(base: site, edited: mine, now: now)
        XCTAssertEqual(rebased.draft.revision, 4)
        XCTAssertEqual(rebased.draft.name, "Musterkogel West ramp")
        XCTAssertEqual(rebased.draft.elevationM, 1880)
        XCTAssertTrue(rebased.draft.windFromDeg.contains(180))
        XCTAssertEqual(rebased.kept, [.height, .wind])
        XCTAssertTrue(rebased.clashes.isEmpty)
        XCTAssertTrue(rebased.clashesInWords.isEmpty)
        XCTAssertEqual(rebased.theirs.count, 1, "\(rebased.theirs)")
        XCTAssertTrue(rebased.words.hasPrefix("Someone else changed this site"), rebased.words)

        // Had the pilot renamed it too: the field both changed comes with
        // the core's sentence to show at it, and the pilot's entry is kept.
        mine.name = "Musterkogel West (upper)"
        let clashing = try siteDraftRebased(base: site, edited: mine, now: now)
        XCTAssertEqual(clashing.clashes, [.name])
        XCTAssertEqual(clashing.clashesInWords.map(\.field), [.name])
        let sentence = try XCTUnwrap(clashing.clashesInWords.first?.words)
        XCTAssertTrue(sentence.hasPrefix("Name: you entered"), sentence)
        XCTAssertTrue(sentence.hasSuffix("Yours is kept."), sentence)
        XCTAssertEqual(clashing.draft.name, "Musterkogel West (upper)")

        // The form's words for correcting follow the site's tier.
        let correcting = try siteCorrecting(site: site)
        XCTAssertTrue(correcting.asPartner)
        XCTAssertEqual(correcting.title, "Correct this site")
        XCTAssertEqual(correcting.note, site.byPilots?.note)
        XCTAssertFalse(correcting.giving.isEmpty)
        XCTAssertFalse(correcting.sendWords.isEmpty)
    }

    /// The server's own example of who says they took off at a site.
    private static let confirmations = #"{"id": "venti:0000000000000000000000s001", "confirmed": {"pilots": 2, "latest_day": "2026-11-16"}, "confirmations": [{"by": {"handle": "mara", "display_name": "Mara Muster"}, "day": "2026-11-16", "note": "North-west at noon, smooth."}, {"by": {"handle": "jonas", "display_name": "Jonas Beispiel"}, "day": "2026-11-09"}]}"#

    func testConfirmationsAreReadWithoutAnAccountAndSaidAndTakenBackWithOne() async throws {
        let id = "venti:0000000000000000000000s001"
        let site = try sitePage(status: 200, body: Data(Self.written.utf8)).site
        XCTAssertEqual(site.id, id)
        let list = try await editing(answering: 200, Self.confirmations).confirmations(of: site)
        XCTAssertEqual(list.notWholeWords, "")
        XCTAssertEqual(list.pilots, 2)
        XCTAssertEqual(list.words, "2 pilots took off here, last on 16 November 2026")
        XCTAssertEqual(list.lines.count, 2)
        XCTAssertTrue(list.lines[0].words.contains("North-west at noon, smooth."), list.lines[0].words)

        // Said: the request the core made goes out signed, and the answer
        // is the site with the core's sentence for the day.
        let sent = Sent()
        let core = try Core(
            dataFolder: FileManager.default.temporaryDirectory
                .appendingPathComponent("venti-site-test-\(UUID().uuidString)", isDirectory: true).path
        )
        let request = try core.siteConfirmRequest(
            id: id, day: "2026-11-20", today: "2026-11-21", note: "Smooth at noon.", flight: nil
        )
        let confirmed = try await editing(answering: 200, Self.written, noting: sent)
            .confirm(request, day: "2026-11-20")
        XCTAssertEqual(confirmed.words, "You confirmed this site for 20 Nov 2026.")
        XCTAssertEqual(confirmed.site.id, id)

        // Taken back.
        let words = try await editing(answering: 204, "", noting: sent).withdraw(id)
        XCTAssertFalse(words.isEmpty)
        let requests = await sent.requests
        XCTAssertEqual(requests.map(\.method), ["PUT", "DELETE"])
        XCTAssertTrue(requests.allSatisfy(\.needsToken))
        XCTAssertTrue(String(decoding: requests[0].body ?? Data(), as: UTF8.self).contains("Smooth at noon."))

        // A refusal is the core's words.
        do {
            _ = try await editing(answering: 400, #"{"error": {"code": "invalid_confirmation", "message": "x"}}"#)
                .confirm(request, day: "2026-11-20")
            XCTFail("The server refused, and nothing was thrown.")
        } catch {
            XCTAssertFalse(SiteEditing.words(for: error).isEmpty)
            XCTAssertTrue(error is CoreError)
        }
    }

    func testADayIsNamedAsTheCoreTakesOneOnThisIPhonesCalendar() throws {
        // Noon UTC of 21 November 2026 is that day in every time zone
        // from UTC-11 to UTC+11.
        let day = SiteDay.day(unixSeconds: 1_795_262_400)
        XCTAssertEqual(day.count, 10)
        XCTAssertTrue(day.hasPrefix("2026-11-2"), day)
        // The core takes it for the form's today, and offers the days back
        // from it under their words.
        let form = try siteConfirmForm(today: day)
        XCTAssertEqual(form.days.first?.day, day)
        XCTAssertEqual(form.days.prefix(2).map(\.words), ["Today", "Yesterday"])
        XCTAssertEqual(
            SiteDay.utcOffsetS(on: day),
            Int32(TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: 1_795_262_400)))
        )
    }
}
