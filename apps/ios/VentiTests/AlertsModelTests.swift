import UserNotifications
import VentiCore
import XCTest

@testable import Venti

/// The alarms' plumbing against the real core, with the server played by
/// its own example answers (`docs/api.md`, "Alarms").
@MainActor
final class AlertsModelTests: XCTestCase {
    /// What was sent, and what was notified.
    private final class Noted {
        var requests: [ServerRequest] = []
        var notified: [FiredLine] = []
        var askedToNotify = 0
    }

    /// The moment of the server's examples: 18 November 2026, 19:05 UTC.
    private static let now: Int64 = 1_795_028_700

    /// A core signed in to an account, as the account model leaves it.
    private func signedInCore() async throws -> Core {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-alerts-test-\(UUID().uuidString)", isDirectory: true)
        let core = try Core(dataFolder: folder.path)
        _ = try core.signedIn(server: "https://venti.example.org", status: 201, body: Data(Answers.signIn.utf8))
        return core
    }

    /// A model whose server answers as the development server's demo
    /// account does, and takes every new alarm.
    private func model(core: Core, noting noted: Noted, fired: String = Answers.fired) -> AlertsModel {
        AlertsModel(
            core: core,
            send: { request in
                noted.requests.append(request)
                let (status, body): (UInt16, String) =
                    switch (request.method, request.path.split(separator: "?").first.map(String.init) ?? "") {
                    case ("GET", "/v1/account/alerts"): (200, Answers.list)
                    case ("GET", "/v1/account/alerts/fired"): (200, fired)
                    case ("POST", "/v1/account/alerts"), ("PUT", _): (201, Answers.saved)
                    case ("DELETE", _): (204, "")
                    default: (404, #"{"error": {"code": "not_found", "message": "x"}}"#)
                    }
                return AccountModel.Answer(status: status, body: Data(body.utf8), retryAfter: nil)
            },
            isSignedIn: { true },
            now: { Self.now },
            notify: { noted.notified += $0 },
            askToNotify: { noted.askedToNotify += 1 }
        )
    }

    func testTheSyncBringsTheAlarmsAndWhatFiredAndNotifiesOnlyWhatIsNew() async throws {
        let core = try await signedInCore()
        let noted = Noted()
        let alerts = model(core: core, noting: noted)
        XCTAssertTrue(alerts.alerts.isEmpty)

        // After signing in: everything Venti keeps is new, and nothing
        // is notified.
        let first = await alerts.sync()
        XCTAssertEqual(first.count, 1)
        XCTAssertTrue(noted.notified.isEmpty)
        XCTAssertEqual(alerts.alerts.map(\.name), ["Patscherkofel north", "Nordkette cross-country"])
        XCTAssertEqual(alerts.fired.count, 1)
        XCTAssertTrue(alerts.fired[0].line.contains("Patscherkofel"), alerts.fired[0].line)
        XCTAssertNil(alerts.problem)
        XCTAssertEqual(noted.requests.map(\.method), ["GET", "GET"])
        XCTAssertTrue(noted.requests.allSatisfy(\.needsToken))

        // Coming to the front: nothing new, nothing notified.
        let empty = Answers.fired.replacingOccurrences(of: "\"fired\": [", with: "\"fired\": [], \"x\": [")
        let again = model(core: core, noting: noted, fired: empty)
        let none = await again.sync(notifies: true)
        XCTAssertTrue(none.isEmpty)
        XCTAssertTrue(noted.notified.isEmpty)
        // What the iPhone holds stays, also for a model made anew.
        XCTAssertEqual(again.fired.count, 1)
        XCTAssertEqual(again.alerts.count, 2)
    }

    func testANewLineComingToTheFrontIsNotified() async throws {
        let core = try await signedInCore()
        let noted = Noted()
        let alerts = model(core: core, noting: noted)
        await alerts.sync(notifies: true)
        XCTAssertEqual(noted.notified.map(\.title), ["Patscherkofel north"])
    }

    func testTheFirstAlarmSavedAsksToNotifyAndFetchesAgain() async throws {
        let core = try await signedInCore()
        let noted = Noted()
        let alerts = model(core: core, noting: noted)
        var draft = AlertsModel.draft(for: .zugspitze)
        XCTAssertEqual(draft.name, "Zugspitze")
        XCTAssertEqual(draft.stationName, "Zugspitze")
        // Neither directions nor speeds: the core says so at its field.
        XCTAssertEqual(alerts.faults(of: draft).map(\.field), ["rule.directions"])
        draft.fromPoint = 0
        XCTAssertTrue(alerts.faults(of: draft).isEmpty)
        XCTAssertEqual(alertLine(draft: draft), "Wind at Zugspitze from N")

        let refused = await alerts.save(draft, id: nil)
        XCTAssertNil(refused)
        XCTAssertEqual(noted.askedToNotify, 1)
        XCTAssertEqual(noted.requests.first?.method, "POST")
        XCTAssertEqual(noted.requests.dropFirst().map(\.method), ["GET", "GET"])

        // A change asks nothing.
        let changed = await alerts.set(try XCTUnwrap(alerts.alerts.first), enabled: false)
        XCTAssertNil(changed)
        XCTAssertEqual(noted.askedToNotify, 1)
        XCTAssertTrue(noted.requests.contains { $0.method == "PUT" })

        let removed = await alerts.remove("0000000000000000000000a001")
        XCTAssertNil(removed)
        XCTAssertTrue(noted.requests.contains { $0.method == "DELETE" })
    }

    func testADayAlarmForATakeoffNamesItsPlace() throws {
        let site = try SiteView.sample
        let draft = AlertsModel.draft(for: site)
        XCTAssertEqual(draft.kind, .day)
        XCTAssertEqual(draft.placeName, site.name)
        XCTAssertEqual(draft.siteId, site.id)
        XCTAssertEqual(draft.daysAhead, Data([0, 1]))
        XCTAssertTrue(alertCheck(draft: draft).isEmpty)
        XCTAssertEqual(alertLine(draft: draft), "A cross-country day or better at \(site.name), today or tomorrow")
    }

    func testATapOnAnAlarmsNotificationOpensAlerts() {
        AlertNotices.taps.opensAlerts = false
        let ofAnAlarm = AlertNotices.identifier(alertId: "a1", firedAtUnixS: Self.now)

        // Another notification's tap, and an alarm's that was swiped away,
        // open nothing.
        AlertNotices.tapped(identifier: "widget-refresh", action: UNNotificationDefaultActionIdentifier)
        XCTAssertFalse(AlertNotices.taps.opensAlerts)
        AlertNotices.tapped(identifier: ofAnAlarm, action: UNNotificationDismissActionIdentifier)
        XCTAssertFalse(AlertNotices.taps.opensAlerts)

        // A tap on what an alarm fired asks for the alarms' page, and the
        // request is kept until a screen takes it: the tap that starts
        // the app comes before there is one.
        AlertNotices.tapped(identifier: ofAnAlarm, action: UNNotificationDefaultActionIdentifier)
        XCTAssertTrue(AlertNotices.taps.opensAlerts)
        AlertNotices.taps.opensAlerts = false

        // Which is a page of the Today tab, wherever the pilot was.
        let navigation = AppNavigation()
        navigation.tab = .logbook
        navigation.showAlerts()
        XCTAssertEqual(navigation.tab, .today)
        XCTAssertTrue(navigation.wantsAlerts)
    }

    func testSignedOutNothingIsAskedAndNothingHeld() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-alerts-test-\(UUID().uuidString)", isDirectory: true)
        let core = try Core(dataFolder: folder.path)
        let noted = Noted()
        let alerts = AlertsModel(
            core: core,
            send: { request in
                noted.requests.append(request)
                return AccountModel.Answer(status: 500, body: Data(), retryAfter: nil)
            },
            isSignedIn: { false }, now: { Self.now }, notify: { noted.notified += $0 }, askToNotify: {}
        )
        let new = await alerts.sync(notifies: true)
        XCTAssertTrue(new.isEmpty)
        XCTAssertTrue(noted.requests.isEmpty)
        XCTAssertTrue(alerts.alerts.isEmpty)
        XCTAssertTrue(alerts.fired.isEmpty)
    }
}

private extension WindStation {
    /// A station as the wind's list has it.
    static let zugspitze = WindStation(
        id: "dwd-10min:10961", name: "Zugspitze", source: "dwd-10min", position: Coordinate(lat: 47.42, lon: 10.98),
        elevationM: 2_956,
        latest: WindReading(
            timeUnixS: 1_000, speedMs: 4, gustMs: nil, directionDeg: 280, temperatureC: nil, humidityPct: nil
        ),
        ageMin: 10, isOld: false, mark: WindMark(symbol: .calm, headingDeg: 0, label: "14")
    )
}

private extension SiteView {
    /// A take-off as the sites' list has it.
    static var sample: SiteView {
        get throws {
            let map = try sitesMap(
                south: 46, west: 9, north: 49, east: 18, status: 200, body: Data(AlertsModelTests.Answers.sites.utf8)
            )
            return try XCTUnwrap(map.sites.first { $0.name == "Wallberg Süd" })
        }
    }
}

extension AlertsModelTests {
    /// Answers of the Venti server, in its own format.
    enum Answers {
        static let signIn = """
            {"account": {"handle": "demo", "display_name": "Demo Pilot", "created_at": "2026-11-18T19:05:00Z"},
             "session": {"id": "session0000000000000000000a", "created_at": "2026-11-18T19:05:00Z",
                         "last_used_at": "2026-11-18T19:05:00Z", "current": true},
             "token": "venti_test_token"}
            """

        static let list = """
            {"alerts": [
              {"id": "0000000000000000000000a001", "name": "Patscherkofel north", "enabled": true,
               "rule": {"kind": "wind", "station": "geosphere-tawes:11126", "station_name": "Patscherkofel",
                        "directions": {"from_deg": 349, "to_deg": 56}, "min_kmh": 10, "max_kmh": 20,
                        "gusts_max_kmh": 30, "hours": {"from_hour": 10, "to_hour": 16}},
               "created_at": "2026-11-16T19:05:00Z", "changed_at": "2026-11-16T19:05:00Z"},
              {"id": "0000000000000000000000a002", "name": "Nordkette cross-country", "enabled": true,
               "rule": {"kind": "day", "place": {"lat": 47.3, "lon": 11.4}, "place_name": "Nordkette",
                        "days_ahead": [0, 1, 2], "at_least": "cross_country"},
               "created_at": "2026-11-16T19:05:00Z", "changed_at": "2026-11-16T19:05:00Z"}
            ]}
            """

        static let saved = """
            {"alert": {"id": "0000000000000000000000a003", "name": "Zugspitze", "enabled": true,
             "rule": {"kind": "wind", "station": "dwd-10min:10961", "station_name": "Zugspitze",
                      "directions": {"from_deg": 349, "to_deg": 11}},
             "created_at": "2026-11-18T19:05:00Z", "changed_at": "2026-11-18T19:05:00Z"}}
            """

        static let fired = """
            {"fired": [
              {"alert_id": "0000000000000000000000a001", "alert_name": "Patscherkofel north",
               "fired_at": "2026-11-18T10:23:00Z",
               "rule": {"kind": "wind", "station": "geosphere-tawes:11126", "station_name": "Patscherkofel",
                        "directions": {"from_deg": 349, "to_deg": 56}, "min_kmh": 10, "max_kmh": 20,
                        "gusts_max_kmh": 30, "hours": {"from_hour": 10, "to_hour": 16}},
               "because": {"kind": "wind", "time": "2026-11-18T10:20:00Z", "speed_kmh": 15, "gust_kmh": 22,
                           "direction_deg": 10}}
            ], "cursor": "1794997380000000", "more": false, "reset": false}
            """

        static let sites = """
            {"layers": [{
                "source": "osm-free-flying", "fetched_at": "2026-11-19T18:10:00Z", "as_of": "2026-11-18T21:21:02Z",
                "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
                "sites": [
                    {"id": "osm-free-flying:node/1", "name": "Wallberg Süd",
                     "position": {"lat": 47.6652, "lon": 11.7954}, "elevation_m": 1620.0, "uses": ["takeoff"],
                     "url": "https://www.openstreetmap.org/node/1"}
                ]
            }],
            "attributions": [{
                "source": "osm-free-flying", "provider": "OpenStreetMap contributors", "licence": "ODbL 1.0",
                "text": "© OpenStreetMap contributors", "url": "https://www.openstreetmap.org/copyright",
                "notice": "Anyone can correct it."
            }]}
            """
    }
}
