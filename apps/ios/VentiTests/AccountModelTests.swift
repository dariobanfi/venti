import UIKit
import VentiCore
import XCTest

@testable import Venti

/// The account model against the real core, with the server played by
/// canned answers in the server's own format.
@MainActor
final class AccountModelTests: XCTestCase {
    /// What the model sent.
    private actor Sent {
        var requests: [URLRequest] = []
        func add(_ request: URLRequest) { requests.append(request) }
    }

    /// The token as the Keychain would hold it.
    private final class TokenBox: @unchecked Sendable {
        var token: String?
    }

    private func makeCore() throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-account-test-\(UUID().uuidString)", isDirectory: true)
        return try Core(dataFolder: folder.path)
    }

    private func sampleText() throws -> String {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "sample", withExtension: "igc"))
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// A server that signs anyone in, has no flights and takes every upload.
    private func server(noting sent: Sent) -> AccountModel.Transport {
        { request in
            await sent.add(request)
            let path = request.url?.path ?? ""
            switch (request.httpMethod ?? "", path) {
            case ("POST", "/v1/sessions"):
                return (201, Data(ServerAnswers.signIn.utf8), nil)
            case ("GET", "/v1/flights/changes"):
                return (200, Data(ServerAnswers.noChanges.utf8), nil)
            case ("GET", "/v1/account/kept-sites/changes"):
                return (200, Data(ServerAnswers.noKeptSites.utf8), nil)
            case ("POST", "/v1/flights"):
                return (201, Data(ServerAnswers.upload.utf8), nil)
            case ("PATCH", _):
                return (200, Data(ServerAnswers.upload.replacingOccurrences(of: "\"private\"", with: "\"unlisted\"").utf8), nil)
            case ("DELETE", _):
                return (204, Data(), nil)
            default:
                return (404, Data(#"{"error": {"code": "not_found", "message": "x"}}"#.utf8), nil)
            }
        }
    }

    func testAFlightSharedLiveGoesToTheServerWithTheToken() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )

        // A flight shared with a link, twenty seconds into the air.
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-live-test-\(UUID().uuidString)", isDirectory: true)
        let session = FlightSession(dataFolder: folder.path)
        let shared = try session.shareLive(
            server: "https://venti.example.org", random: Data(0..<26), audience: .link
        )
        XCTAssertEqual(shared.link, "https://venti.example.org/live/0123456789abcdefghjkmnpqrs")
        XCTAssertEqual(shared.words, "Begins at take-off")
        var request: ServerRequest?
        for second in 0..<20 where request == nil {
            session.onLocation(sample: LocationSample(
                timeUnixS: 1_795_028_700 + Double(second), lat: 47.66 + Double(second) * 0.0001, lon: 11.80,
                altitudeM: 1600 - Double(second), groundSpeedMs: 11, courseDeg: 0
            ))
            request = session.liveRequest(nowUnixS: 1_795_028_700 + Double(second))
        }
        let positions = try XCTUnwrap(request)

        // Without an account there is nobody to carry it to.
        let none = await account.carry(positions)
        XCTAssertNil(none)

        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        let came = await account.carry(positions)
        let answer = try XCTUnwrap(came)
        let last = await sent.requests.last
        let carried = try XCTUnwrap(last)
        XCTAssertEqual(carried.url?.absoluteString, "https://venti.example.org/v1/live/positions")
        XCTAssertEqual(carried.httpMethod, "POST")
        XCTAssertEqual(carried.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(carried.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(carried.timeoutInterval, 20)
        XCTAssertNotNil(carried.httpBody)

        // This made-up server is one from before live tracks: the core
        // puts its answer into words and sends nothing more.
        XCTAssertEqual(answer.status, 404)
        session.liveAnswer(
            status: answer.status, body: answer.body, retryAfterS: answer.retryAfter, nowUnixS: 1_795_028_730
        )
        let stopped = try XCTUnwrap(session.liveShare(nowUnixS: 1_795_028_730))
        XCTAssertEqual(stopped.state, .stopped)
        XCTAssertEqual(stopped.words, "This server has no live tracking.")
        XCTAssertNil(session.liveRequest(nowUnixS: 1_795_028_800))

        // Signed out again, the request is answered as a server would.
        box.token = nil
        let without = await account.carry(positions)
        let refused = try XCTUnwrap(without)
        XCTAssertEqual(refused.status, 401)
    }

    func testSigningInKeepsTheTokenAndSendsTheLogbook() async throws {
        let core = try makeCore()
        let flight = try core.importFlight(igcText: try sampleText()).id
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: server(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        XCTAssertFalse(account.isSignedIn)
        // No sync has run: the screen has no "Last sync" line.
        XCTAssertNil(account.lastSyncWords)

        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")

        XCTAssertNil(account.problem)
        XCTAssertTrue(account.isSignedIn)
        XCTAssertEqual(account.account?.handle, "demo")
        XCTAssertEqual(box.token, "venti_test_token")
        // Signing in is followed by a sync, which sent the one flight.
        XCTAssertEqual(account.lastSync?.sent, 1)
        // And the account screen's "Last sync" says so in the core's words.
        XCTAssertEqual(account.lastSyncWords, "1 sent")
        XCTAssertEqual(account.lastSyncWords, account.lastSync.map { syncSummaryInWords(summary: $0) })
        XCTAssertEqual(account.account?.flightsToSend, 0)

        let requests = await sent.requests
        // The logbook's run is followed by the run of the pilot's own sites.
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["POST", "GET", "POST", "GET"])
        XCTAssertEqual(requests[3].url?.path, "/v1/account/kept-sites/changes")
        XCTAssertTrue(requests.allSatisfy { $0.url?.host() == "venti.example.org" })
        // The password goes with the sign-in and the token does not; after
        // that the token goes with every request.
        XCTAssertNil(requests[0].value(forHTTPHeaderField: "Authorization"))
        XCTAssertEqual(requests[1].value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(requests[2].value(forHTTPHeaderField: "Content-Type"), "application/octet-stream")
        XCTAssertEqual(requests[2].httpBody, Data(try sampleText().utf8))

        // Sharing changes who may see the flight and gives its link.
        XCTAssertEqual(account.sharing(of: flight)?.visibility, .private)
        let shared = await account.share(flight, with: .unlisted)
        XCTAssertEqual(shared?.visibility, .unlisted)
        XCTAssertEqual(shared?.link, "https://venti.example.org/f/wkvydb8am8wmdcxzds9vkrw5kr")

        // Signing out tells the server and forgets the token; the flight stays.
        await account.signOut()
        XCTAssertNil(account.account)
        XCTAssertNil(box.token)
        XCTAssertEqual(core.logbook().totals.flights, 1)
        let last = await sent.requests.last
        XCTAssertEqual(last?.httpMethod, "DELETE")
        XCTAssertEqual(last?.url?.path, "/v1/sessions/session0000000000000000000a")
    }

    func testAKeychainThatRefusesTheSessionIsSaidAndNoSessionIsLeftBehind() async throws {
        let core = try makeCore()
        let sent = Sent()
        // A Keychain that keeps nothing, as an app without an identity has.
        let account = AccountModel(
            core: core, transport: server(noting: sent), readToken: { nil }, saveToken: { _ in }
        )

        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")

        XCTAssertFalse(account.isSignedIn)
        XCTAssertNil(account.account)
        XCTAssertNil(try core.account())
        XCTAssertEqual(
            account.problem,
            "This iPhone could not keep the session in its Keychain, so you are not signed in. Restart the iPhone and try again."
        )
        // The server was told to forget the session, with the session's own
        // token, and nothing else was asked of it.
        let requests = await sent.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["POST", "DELETE"])
        XCTAssertEqual(requests[1].url?.path, "/v1/sessions/session0000000000000000000a")
        XCTAssertEqual(requests[1].value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
    }

    func testAWrongPasswordIsSaidAndNothingIsKept() async throws {
        let box = TokenBox()
        let account = AccountModel(
            core: try makeCore(),
            transport: { _ in
                (401, Data(#"{"error": {"code": "invalid_credentials", "message": "x"}}"#.utf8), nil)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo", password: "wrong"
        )
        XCTAssertEqual(account.problem, "The handle or the password is wrong.")
        XCTAssertFalse(account.isSignedIn)
        XCTAssertNil(box.token)
    }

    /// What Venti says of the account is asked for and kept by the core,
    /// and what a site offers and whether a place may be added follow it.
    func testWhatVentiSaysOfTheAccountDecidesWhatASiteOffers() async throws {
        final class Answer: @unchecked Sendable {
            var status = 200
            var body = #"{"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z", "edits_sites": true}}"#
        }
        let core = try makeCore()
        let (sent, box, answer) = (Sent(), TokenBox(), Answer())
        let plain = server(noting: sent)
        let account = AccountModel(
            core: core,
            transport: { request in
                if request.httpMethod == "GET", request.url?.path == "/v1/account" {
                    await sent.add(request)
                    return (answer.status, Data(answer.body.utf8), nil)
                }
                return try await plain(request)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        // A partner's take-off, as the server hands it out.
        let site = try sitePage(
            status: 200,
            body: Data(#"{"site": {"id": "venti:0000000000000000000000s001", "name": "Musterkogel West", "position": {"lat": 47.3, "lon": 10.9}, "uses": ["takeoff"]}, "standing": {"id": "venti:0000000000000000000000s001", "revision": 1, "created_at": "2026-10-29T19:05:00Z", "changed_at": "2026-10-29T19:05:00Z"}, "lasting": true, "attributions": []}"#.utf8)
        ).site
        XCTAssertEqual(site.byPilots?.label, "From a partner")

        // Signed out: nothing is asked, nothing is added, and the core's
        // line says why.
        await account.read()
        let none = await sent.requests
        XCTAssertTrue(none.isEmpty)
        let signedOut = try XCTUnwrap(account.adding(takesSitesFromPilots: true))
        XCTAssertFalse(signedOut.mayAdd)
        // The entry is the sentence about signing in, and leads there.
        XCTAssertEqual(signedOut.entryWords, "Sign in to add a place pilots take off from.")
        XCTAssertTrue(signedOut.leadsToSignIn)
        // Where Venti takes none there is no entry, and a line says why.
        let taken = try XCTUnwrap(account.adding(takesSitesFromPilots: false))
        XCTAssertEqual(taken.entryWords, "")
        XCTAssertFalse(taken.leadsToSignIn)
        XCTAssertFalse(taken.whyNot.isEmpty)
        let closed = try XCTUnwrap(account.offers(for: site, takesSitesFromPilots: true, confirmations: nil))
        XCTAssertFalse(closed.correct.isOffered)
        XCTAssertFalse(closed.confirm.isOffered)
        XCTAssertTrue(closed.history.isOffered)
        XCTAssertFalse(closed.note.isEmpty)

        // Signed in as an ordinary pilot: a place from pilots may be
        // added, and a partner's site is not this pilot's to correct.
        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        XCTAssertEqual(account.account?.mayEditPartnerSites, false)
        let pilot = try XCTUnwrap(account.adding(takesSitesFromPilots: true))
        XCTAssertTrue(pilot.mayAdd)
        XCTAssertFalse(pilot.asPartner)
        XCTAssertEqual(pilot.entryWords, "Add a place pilots take off from")
        XCTAssertEqual(pilot.sendWords, "Add as a place from pilots")
        let asPilot = try XCTUnwrap(account.offers(for: site, takesSitesFromPilots: true, confirmations: nil))
        XCTAssertFalse(asPilot.correct.isOffered)
        XCTAssertFalse(asPilot.remove.isOffered)
        XCTAssertTrue(asPilot.confirm.isOffered)
        XCTAssertEqual(asPilot.confirm.words, "I took off here and it was good")
        XCTAssertTrue(asPilot.report.isOffered)
        XCTAssertTrue(asPilot.note.hasPrefix("A partner of Venti added this site"), asPilot.note)

        // Venti says the account is a partner: asked with the token, kept,
        // and the site is the account's to correct.
        await account.read()
        let asked = await sent.requests.last
        XCTAssertEqual(asked?.url?.path, "/v1/account")
        XCTAssertEqual(asked?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(account.account?.mayEditPartnerSites, true)
        XCTAssertEqual(account.adding(takesSitesFromPilots: true)?.asPartner, true)
        let asPartner = try XCTUnwrap(account.offers(for: site, takesSitesFromPilots: true, confirmations: nil))
        XCTAssertTrue(asPartner.correct.isOffered)
        XCTAssertEqual(asPartner.correct.words, "Correct this site")

        // No answer: the account is as it was last said.
        answer.status = 503
        answer.body = ""
        await account.read()
        XCTAssertEqual(account.account?.mayEditPartnerSites, true)
        XCTAssertTrue(account.isSignedIn)

        // The session ended on the server: no error, the account says so,
        // and the token goes.
        answer.status = 401
        answer.body = #"{"error": {"code": "unauthorized", "message": "x"}}"#
        await account.read()
        XCTAssertFalse(account.isSignedIn)
        XCTAssertEqual(account.account?.needsSignIn, true)
        XCTAssertNil(box.token)
        XCTAssertNil(account.problem)
    }

    func testAnEndedSessionAsksThePilotToSignInAgain() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let good = server(noting: sent)
        let hasEnded = TokenBox()
        let account = AccountModel(
            core: core,
            transport: { request in
                if hasEnded.token != nil, request.url?.path == "/v1/flights/changes" {
                    return (401, Data(#"{"error": {"code": "unauthorized", "message": "x"}}"#.utf8), nil)
                }
                return try await good(request)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo",
            password: "demo-pilot-password"
        )
        XCTAssertTrue(account.isSignedIn)

        hasEnded.token = "yes"
        await account.sync(force: true)

        XCTAssertFalse(account.isSignedIn)
        XCTAssertEqual(account.account?.needsSignIn, true)
        XCTAssertNil(box.token)
        XCTAssertEqual(account.problem, "This device is no longer signed in. Sign in again.")
    }

    /// Waits until something that an unawaited task brings about is so.
    private func eventually(_ what: String, _ isSo: () async -> Bool) async throws {
        for _ in 0..<200 {
            if await isSo() {
                return
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTFail("Never came about: \(what)")
    }

    func testThePilotsOwnSitesFollowTheAccount() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let good = server(noting: sent)
        let account = AccountModel(
            core: core,
            transport: { request in
                // The account keeps the Jägerkamp; after the first read
                // nothing has changed.
                if request.url?.path == "/v1/account/kept-sites/changes",
                   !(request.url?.query ?? "").contains("since=")
                {
                    await sent.add(request)
                    return (200, Data(ServerAnswers.keptJaegerkamp.utf8), nil)
                }
                return try await good(request)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        let own = KeptSitesModel(core: core)
        account.keepsInStep(own)

        // Signed out the list is the iPhone's alone, and nothing is asked.
        await own.load(from: nil)
        XCTAssertEqual(own.list?.sites.count, 0)
        XCTAssertTrue(own.list?.note.hasPrefix("Kept on this device only") ?? false, own.list?.note ?? "")
        await account.sync(force: true)
        let before = await sent.requests
        XCTAssertTrue(before.isEmpty)

        // Signing in runs the logbook's sync and beside it the sync of
        // the pilot's own sites, with the session: the account's site is
        // on the iPhone, and the note says where the list lives.
        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        XCTAssertNil(account.problem)
        try await eventually("the account's site on the iPhone") { own.list?.sites.count == 1 }
        let kept = try XCTUnwrap(own.list?.sites.first)
        XCTAssertEqual(kept.site.name, "Fluggelände Jägerkamp/Nagelspitz")
        XCTAssertTrue(own.list?.note.hasPrefix("Kept on your Venti account and on this device.") ?? false)
        // Today and the list read again: the sites on the iPhone changed.
        XCTAssertEqual(own.changes, 1)
        let asked = await sent.requests.filter { $0.url?.path == "/v1/account/kept-sites/changes" }
        XCTAssertEqual(asked.count, 1)
        XCTAssertEqual(asked.first?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")

        // Forgotten while signed in: the account is told at once.
        try await own.forget(kept.site.id)
        XCTAssertEqual(own.list?.sites.count, 0)
        try await eventually("the account told of the forgotten site") {
            await sent.requests.contains {
                $0.httpMethod == "DELETE" && ($0.url?.path ?? "").hasPrefix("/v1/account/kept-sites/")
            }
        }
        try await eventually("the run at its end") { !account.isWorking }
        let forgotten = await sent.requests.last { $0.httpMethod == "DELETE" }
        XCTAssertEqual(forgotten?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")

        // Signed out, keeping is the iPhone's matter again: nothing is sent.
        await account.signOut()
        try await eventually("the note of a list on the iPhone alone") {
            own.list?.note.hasPrefix("Kept on this device only") ?? false
        }
        let count = await sent.requests.count
        try await own.keep(kept.site, credits: [], asOfUnixS: kept.asOfUnixS)
        try await Task.sleep(for: .milliseconds(200))
        let after = await sent.requests.count
        XCTAssertEqual(after, count)
        XCTAssertEqual(own.list?.sites.count, 1)
    }

    func testAReportGoesToTheServerFromTheAccountAndSaysWhatBecameOfIt() async throws {
        let (sent, box) = (Sent(), TokenBox())
        let good = server(noting: sent)
        let account = AccountModel(
            core: try makeCore(),
            transport: { request in
                if request.url?.path == "/v1/reports" {
                    await sent.add(request)
                    return (201, Data(#"{"created": true}"#.utf8), nil)
                }
                return try await good(request)
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )

        // Without an account nothing is sent.
        do {
            _ = try await account.report(.pilot(handle: "mara"), reason: .spam, text: "")
            XCTFail("A report without an account was sent")
        } catch {
            XCTAssertEqual(AccountModel.words(for: error), "This iPhone is no longer signed in. Sign in again.")
        }
        let before = await sent.requests
        XCTAssertTrue(before.isEmpty)

        await account.signIn(
            server: try XCTUnwrap(URL(string: "https://venti.example.org")), handle: "demo",
            password: "demo-pilot-password"
        )
        let outcome = try await account.report(
            .flight(id: "fbyw03atd9qdt5bfe177ra1k94"), reason: .notAFlight, text: " This is my track. "
        )
        XCTAssertEqual(outcome, "Sent. Whoever runs Venti will read it.")

        // Signing in may have set a sync going: only the reports count.
        let reports = await sent.requests.filter { $0.url?.path == "/v1/reports" }
        XCTAssertEqual(reports.count, 1)
        let report = try XCTUnwrap(reports.last)
        XCTAssertEqual(report.httpMethod, "POST")
        XCTAssertEqual(report.url?.absoluteString, "https://venti.example.org/v1/reports")
        XCTAssertEqual(report.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(report.value(forHTTPHeaderField: "Content-Type"), "application/json")
        let body = try XCTUnwrap(report.httpBody)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(json["reason"] as? String, "not_a_flight")
        // Cleaned as the server would clean it.
        XCTAssertEqual(json["text"] as? String, "This is my track.")
        XCTAssertEqual((json["subject"] as? [String: String])?["flight"], "fbyw03atd9qdt5bfe177ra1k94")

        // A report that has to say what is wrong, and does not, is refused
        // before anything is sent.
        do {
            _ = try await account.report(.pilot(handle: "mara"), reason: .other, text: " ")
            XCTFail("A report without words was sent")
        } catch {
            XCTAssertEqual(AccountModel.words(for: error), "Say in a few words what is wrong.")
        }
        let after = await sent.requests.filter { $0.url?.path == "/v1/reports" }
        XCTAssertEqual(after.count, 1)

        // Two addresses of one server are one server.
        XCTAssertTrue(ReportSheet.same("https://venti.example.org", "https://Venti.example.org/"))
        XCTAssertFalse(ReportSheet.same("https://venti.example.org", "https://other.example.org"))
    }

    /// A server that keeps the account's settings as the development
    /// server answers them: its golden answers to a change of the account
    /// and of all flights at once.
    private func settingsServer(noting sent: Sent) -> AccountModel.Transport {
        { request in
            await sent.add(request)
            let body = request.httpBody.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            switch (request.httpMethod ?? "", request.url?.path ?? "") {
            case ("POST", "/v1/sessions"):
                return (201, Data(ServerAnswers.signInNow.utf8), nil)
            case ("GET", "/v1/flights/changes"):
                return (200, Data(ServerAnswers.noChanges.utf8), nil)
            case ("GET", "/v1/account/kept-sites/changes"):
                return (200, Data(ServerAnswers.noKeptSites.utf8), nil)
            case ("PATCH", "/v1/account") where body.contains("new_flights_public"):
                return (200, Data(ServerAnswers.accountSettings.utf8), nil)
            case ("PATCH", "/v1/account"):
                return (200, Data(ServerAnswers.accountDetails.utf8), nil)
            case ("PATCH", "/v1/flights"):
                return (200, Data(ServerAnswers.flightsVisibility.utf8), nil)
            default:
                return (404, Data(#"{"error": {"code": "not_found", "message": "x"}}"#.utf8), nil)
            }
        }
    }

    /// A request's body as JSON, to hold against a golden request.
    private func json(of request: URLRequest?) throws -> NSDictionary {
        let body = try XCTUnwrap(request?.httpBody)
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: body) as? NSDictionary)
    }

    private func json(_ text: String) throws -> NSDictionary {
        try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(text.utf8)) as? NSDictionary)
    }

    func testThePilotSwitchesNewFlightsOffAndMakesAllPublic() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: settingsServer(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        // On for every account until the pilot says otherwise.
        XCTAssertEqual(account.account?.newFlightsPublic, true)
        XCTAssertNil(account.allFlights)

        // The switch: the golden request, and the account its answer holds.
        await account.setNewFlightsPublic(false)
        XCTAssertNil(account.problem)
        let switched = await sent.requests.last
        XCTAssertEqual(switched?.httpMethod, "PATCH")
        XCTAssertEqual(switched?.url?.path, "/v1/account")
        XCTAssertEqual(switched?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(try json(of: switched), try json(ServerAnswers.accountSettingsRequest))
        XCTAssertEqual(account.account?.newFlightsPublic, false)
        XCTAssertFalse(account.isWorking)

        // All public at once: the core's question first, then the golden
        // request, the answer's line, and a sync after it.
        XCTAssertEqual(allFlightsQuestion(flights: 3, visibility: .public), "Make all 3 flights public?")
        let before = await sent.requests.count
        await account.setAllFlights(.public)
        XCTAssertNil(account.problem)
        let requests = await sent.requests
        let all = requests[before]
        XCTAssertEqual(all.httpMethod, "PATCH")
        XCTAssertEqual(all.url?.path, "/v1/flights")
        XCTAssertEqual(try json(of: all), try json(ServerAnswers.flightsVisibilityRequest))
        XCTAssertEqual(account.allFlights?.line, "3 flights are public")
        XCTAssertEqual(account.allFlights?.changed, 2)
        XCTAssertTrue(requests.dropFirst(before + 1).contains { $0.url?.path == "/v1/flights/changes" })

        // Signed out, the line goes with the account.
        await account.signOut()
        XCTAssertNil(account.allFlights)
    }

    func testThePilotsCountryIsSetAndTheGliderClassFollowsTheApp() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let account = AccountModel(
            core: core, transport: settingsServer(noting: sent),
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        // The app's glider setting says EN B: the account is told so at
        // the sign-in, before the logbook is sent.
        var appClass: GliderClassKind? = .b
        account.gliderClass = { appClass }
        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        let golden = try json(ServerAnswers.accountDetailsRequest)
        var requests = await sent.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["POST", "PATCH", "GET", "GET"])
        XCTAssertEqual(try json(of: requests[1]), NSDictionary(dictionary: ["glider_class": golden["glider_class"] as Any]))
        XCTAssertEqual(account.account?.gliderClass, .b)

        // The same class again is not sent again.
        await account.gliderClassChanged(.b)
        await account.read()
        requests = await sent.requests
        XCTAssertEqual(requests.filter { $0.httpMethod == "PATCH" }.count, 1)

        // The country: the core writes the code in capitals.
        await account.setCountry("de")
        XCTAssertNil(account.problem)
        let country = await sent.requests.last
        XCTAssertEqual(country?.url?.path, "/v1/account")
        XCTAssertEqual(try json(of: country), NSDictionary(dictionary: ["country": "DE"]))
        XCTAssertEqual((golden["country"] as? String)?.uppercased(), "DE")
        XCTAssertEqual(account.account?.country, "DE")
        // Text that is no code is the core's to refuse, in words, and
        // nothing is sent.
        let count = await sent.requests.count
        await account.setCountry("Germany")
        XCTAssertEqual(account.problem, "A country is two letters, such as DE or AT.")
        account.problem = nil
        let after = await sent.requests.count
        XCTAssertEqual(after, count)

        // The app's class changed: the account takes it. An iPhone with
        // no class set leaves the account's as it is.
        await account.gliderClassChanged(.c)
        let changed = await sent.requests.last
        XCTAssertEqual(try json(of: changed), NSDictionary(dictionary: ["glider_class": "en_c"]))
        appClass = nil
        let patches = await sent.requests.filter { $0.httpMethod == "PATCH" }.count
        await account.signOut()
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        let again = await sent.requests.filter { $0.httpMethod == "PATCH" }.count
        XCTAssertEqual(again, patches)
    }

    /// The pilot's picture, sent as a JPEG and removed, and the switch for
    /// being among the pilots near a place: each the core's request, and
    /// the account its answer holds.
    func testThePilotsPictureIsSentAndRemovedAndTheNearSwitchIsKept() async throws {
        let core = try makeCore()
        let (sent, box) = (Sent(), TokenBox())
        let withPicture = """
            {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z",
              "new_flights_public": true, "shown_near_places": true,
              "picture": "/v1/pictures/aaaaaaaaaaaaaaaaaaaaaaaaaa"}}
            """
        let noPicture = """
            {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z",
              "new_flights_public": true, "shown_near_places": true}}
            """
        let notNear = """
            {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z",
              "new_flights_public": true, "shown_near_places": false}}
            """
        let settings = settingsServer(noting: sent)
        let account = AccountModel(
            core: core,
            transport: { request in
                switch (request.httpMethod ?? "", request.url?.path ?? "") {
                case ("PUT", "/v1/account/picture"):
                    await sent.add(request)
                    return (200, Data(withPicture.utf8), nil)
                case ("DELETE", "/v1/account/picture"):
                    await sent.add(request)
                    return (200, Data(noPicture.utf8), nil)
                case ("PATCH", "/v1/account") where String(data: request.httpBody ?? Data(), encoding: .utf8)?.contains("shown_near_places") == true:
                    await sent.add(request)
                    return (200, Data(notNear.utf8), nil)
                default:
                    return try await settings(request)
                }
            },
            readToken: { box.token }, saveToken: { box.token = $0 }
        )
        let address = try XCTUnwrap(URL(string: "https://venti.example.org"))
        await account.signIn(server: address, handle: "demo", password: "demo-pilot-password")
        XCTAssertNil(account.account?.picturePath)
        XCTAssertEqual(account.account?.shownNearPlaces, true)

        // A photo of 3,000 by 2,000 goes as a JPEG of 1,000 by 667.
        let photo = UIGraphicsImageRenderer(size: CGSize(width: 3000, height: 2000), format: {
            let format = UIGraphicsImageRendererFormat()
            format.scale = 1
            return format
        }()).pngData { context in
            UIColor.gray.setFill()
            context.fill(CGRect(x: 0, y: 0, width: 3000, height: 2000))
        }
        let jpeg = try XCTUnwrap(PictureUpload.jpeg(photo))
        let sized = try XCTUnwrap(UIImage(data: jpeg))
        XCTAssertEqual(sized.size, CGSize(width: 1000, height: 667))
        XCTAssertEqual(Array(jpeg.prefix(2)), [0xFF, 0xD8])
        XCTAssertNil(PictureUpload.jpeg(Data("no picture".utf8)))

        await account.setPicture(jpeg, mediaType: PictureUpload.mediaType)
        XCTAssertNil(account.problem)
        let put = await sent.requests.last
        XCTAssertEqual(put?.httpMethod, "PUT")
        XCTAssertEqual(put?.value(forHTTPHeaderField: "Content-Type"), "image/jpeg")
        XCTAssertEqual(put?.value(forHTTPHeaderField: "Authorization"), "Bearer venti_test_token")
        XCTAssertEqual(put?.httpBody, jpeg)
        XCTAssertEqual(account.account?.picturePath, "/v1/pictures/aaaaaaaaaaaaaaaaaaaaaaaaaa")

        // A kind the server does not take is refused in the core's words, and
        // nothing is sent.
        let count = await sent.requests.count
        await account.setPicture(Data([1, 2, 3]), mediaType: "image/gif")
        XCTAssertEqual(account.problem, "Choose a photo in JPEG, PNG or WebP.")
        account.problem = nil
        let after = await sent.requests.count
        XCTAssertEqual(after, count)

        await account.removePicture()
        let removal = await sent.requests.last
        XCTAssertEqual(removal?.httpMethod, "DELETE")
        XCTAssertNil(account.account?.picturePath)

        await account.setShownNearPlaces(false)
        let near = await sent.requests.last
        XCTAssertEqual(near?.httpMethod, "PATCH")
        XCTAssertEqual(try json(of: near), NSDictionary(dictionary: ["shown_near_places": false]))
        XCTAssertEqual(account.account?.shownNearPlaces, false)
    }
}

/// Answers of the Venti server, in its own format.
private enum ServerAnswers {
    /// `sign_in.json` as the server answers now, with the setting for new
    /// flights, and the token the other answers here have.
    static let signInNow = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z",
                     "edits_sites": true, "new_flights_public": true},
         "session": {"id": "session0000000000000000000a", "created_at": "2026-11-18T19:05:00Z",
                     "last_used_at": "2026-11-18T19:05:00Z", "current": true},
         "token": "venti_test_token"}
        """

    /// `crates/server/tests/golden/account_settings.json` and the request
    /// it answers.
    static let accountSettings = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z",
          "edits_sites": true, "new_flights_public": false}}
        """
    static let accountSettingsRequest = #"{"new_flights_public": false}"#

    /// `account_details.json` and its request.
    static let accountDetails = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z",
          "edits_sites": true, "new_flights_public": true, "country": "DE", "glider_class": "en_b"}}
        """
    static let accountDetailsRequest = #"{"country": "de", "glider_class": "en_b"}"#

    /// `flights_visibility.json` and its request.
    static let flightsVisibility = #"{"visibility": "public", "flights": 3, "changed": 2, "taken_down": 0}"#
    static let flightsVisibilityRequest = #"{"visibility": "public"}"#

    static let signIn = """
        {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"},
         "session": {"id": "session0000000000000000000a", "created_at": "2026-11-18T19:05:00Z",
                     "last_used_at": "2026-11-18T19:05:00Z", "current": true},
         "token": "venti_test_token"}
        """

    static let noChanges = """
        {"flights": [], "deleted": [], "cursor": "c1", "more": false, "reset": false}
        """

    static let noKeptSites = """
        {"kept": [], "forgotten": [], "cursor": "1795028700000002", "more": false, "reset": false}
        """

    /// The account keeps one site: the server's own example answer.
    static let keptJaegerkamp = """
        {"kept": [{"site": {"id": "osm-free-flying:way/1080986173", "name": "Fluggelände Jägerkamp/Nagelspitz",
          "position": {"lat": 47.6791413, "lon": 11.9051032}, "elevation_m": 1560.0, "uses": ["takeoff"],
          "paragliding": true, "hang_gliding": true, "official": true,
          "url": "https://www.openstreetmap.org/way/1080986173", "website": "https://www.dhv.de/", "fee": "yes"},
          "attribution": {"source": "osm-free-flying", "provider": "OpenStreetMap contributors",
            "licence": "ODbL 1.0", "text": "© OpenStreetMap contributors",
            "url": "https://www.openstreetmap.org/copyright", "notice": "OpenStreetMap's data, under the ODbL."},
          "as_of": "2026-11-18T19:05:00Z", "kept_at": "2026-11-18T17:30:00Z"}],
         "forgotten": [], "cursor": "1795028700000002", "more": false, "reset": false}
        """

    static let upload = """
        {"flight": {"id": "wkvydb8am8wmdcxzds9vkrw5kr", "identity": "d76b1d1a62211781e7fac7e5db815546",
          "pilot": {"handle": "demo", "display_name": "Demo Pilot"}, "visibility": "private",
          "uploaded_at": "2026-11-18T19:05:00Z", "start": "2026-09-01T10:30:00Z", "airtime_s": 1744,
          "track_length_m": 18402.3, "max_altitude_m": 2117, "max_climb_ms": 2.4,
          "outline": [[47.6642, 11.7967], [47.6697, 11.78986]], "file_bytes": 64678, "analysis_version": 1},
         "created": true}
        """
}
