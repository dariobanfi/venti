import XCTest

/// Drives the app the way a pilot would, on a simulator, and keeps a
/// screenshot of every screen. The CI workflow publishes the screenshots, so
/// a change to the interface can be looked at without a Mac.
///
/// The app is started with an empty logbook in a throwaway folder.
@MainActor
final class VentiUITests: XCTestCase {
    private func launch(bundledMap: Bool, dark: Bool = false, extra: [String] = []) -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments =
            ["--ui-testing"] + (bundledMap ? ["--bundled-map"] : []) + (dark ? ["--dark"] : []) + extra
        app.launch()
        return app
    }

    private func keepScreenshot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// Anything on screen with this identifier or label.
    private func element(_ name: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)[name].firstMatch
    }

    /// Changes to a tab. After a screen that covered the tab bar, and just
    /// after launch, the bar is still settling and a tap goes nowhere: wait
    /// until the button can be hit, tap, and tap again if the tab did not
    /// become the selected one.
    private func openTab(_ name: String, in app: XCUIApplication) {
        let tab = app.tabBars.buttons[name]
        XCTAssertTrue(tab.waitForExistence(timeout: 15))
        for _ in 0..<4 {
            for _ in 0..<40 where !tab.isHittable {
                usleep(250_000)
            }
            usleep(400_000)
            tab.tap()
            for _ in 0..<8 where !tab.isSelected {
                usleep(250_000)
            }
            if tab.isSelected {
                return
            }
        }
        XCTFail("The \(name) tab did not open.")
    }

    /// Anything on screen whose accessibility value contains this text.
    private func valued(containing text: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "value CONTAINS %@", text))
            .firstMatch
    }

    /// Anything on screen that says this, as its name or as its value.
    private func saying(_ text: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", text, text))
            .firstMatch
    }

    private func labelled(containing text: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "label CONTAINS %@", text))
            .firstMatch
    }

    /// From an empty logbook to a flight on the map and back to empty.
    private func addSampleFlightAndOpenIt(in app: XCUIApplication, prefix: String) {
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        // Give the map a moment to draw.
        sleep(4)
        keepScreenshot("\(prefix)-1-map")

        openTab("Logbook", in: app)
        XCTAssertTrue(app.staticTexts["No flights yet"].waitForExistence(timeout: 15))
        keepScreenshot("\(prefix)-2-logbook-empty")

        app.buttons["Add a sample flight"].tap()
        // A single imported flight opens straight away.
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Airtime", in: app).exists)
        sleep(6)
        keepScreenshot("\(prefix)-3-flight")
    }

    func testAPilotImportsAFlightInspectsItAndDeletesIt() {
        let app = launch(bundledMap: true)
        addSampleFlightAndOpenIt(in: app, prefix: "light")

        // Dragging along the altitude chart marks a moment, which stays.
        let chart = element("altitude-chart", in: app)
        XCTAssertTrue(chart.exists)
        chart.coordinate(withNormalizedOffset: CGVector(dx: 0.15, dy: 0.5))
            .press(forDuration: 0.3, thenDragTo: chart.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)))
        XCTAssertTrue(app.buttons["Show flight totals"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("In the air", in: app).exists)
        sleep(1)
        keepScreenshot("light-4-flight-moment")

        app.buttons["Show flight totals"].tap()
        XCTAssertTrue(labelled(containing: "Best climb", in: app).waitForExistence(timeout: 10))

        app.buttons["Flight details"].tap()
        XCTAssertTrue(app.navigationBars["Flight details"].waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("light-5-flight-details")

        // Choosing a thermal from the list marks it on the map.
        let thermal = element("thermal-row", in: app)
        for _ in 0..<4 where !thermal.exists {
            app.swipeUp()
        }
        XCTAssertTrue(thermal.waitForExistence(timeout: 15))
        keepScreenshot("light-5b-flight-thermals")
        thermal.tap()
        XCTAssertTrue(app.buttons["Show flight totals"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("In the air", in: app).exists)
        app.buttons["Show flight totals"].tap()

        // Back to the logbook, where the flight is now listed.
        app.navigationBars.buttons.firstMatch.tap()
        let row = element("logbook-row", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("light-6-logbook")

        // The totals lead to the statistics.
        element("logbook-totals", in: app).tap()
        XCTAssertTrue(app.staticTexts["Your bests"].waitForExistence(timeout: 15) || element("Longest flight", in: app).exists)
        sleep(1)
        keepScreenshot("light-6b-statistics")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(row.waitForExistence(timeout: 15))

        // The map tab now shows the flight too.
        openTab("Map", in: app)
        sleep(4)
        keepScreenshot("light-7-map-with-flight")
        openTab("Logbook", in: app)

        row.swipeLeft()
        app.buttons["Delete"].tap()
        let confirm = app.buttons["Delete flight"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 10))
        keepScreenshot("light-8-delete")
        confirm.tap()
        XCTAssertTrue(app.staticTexts["No flights yet"].waitForExistence(timeout: 15))
    }

    func testTheAppInDarkAppearance() {
        let app = launch(bundledMap: true, dark: true)
        addSampleFlightAndOpenIt(in: app, prefix: "dark")

        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("dark-4-logbook")
    }

    /// The cockpit, fed by a replay of the sample flight at twenty times
    /// its speed: about ninety seconds in the glider starts to circle in
    /// lift, and four minutes later it glides on.
    func testTheCockpitShowsAReplayedFlight() {
        let app = launch(bundledMap: true, extra: ["--sample-waypoints"])
        openTab("Fly", in: app)
        XCTAssertTrue(app.buttons["Start flight"].waitForExistence(timeout: 30))
        keepScreenshot("fly-1-ready")

        app.buttons["Replay the sample flight"].tap()
        app.buttons["20 times faster"].tap()
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 20))

        // Circling in lift, the cockpit changes to its thermal numbers by
        // itself.
        XCTAssertTrue(element("Gained", in: app).waitForExistence(timeout: 40))
        sleep(3)
        keepScreenshot("fly-2-thermal")

        // On the glide after the thermal it changes back, now with the wind
        // it worked out while circling.
        XCTAssertTrue(element("Glide", in: app).waitForExistence(timeout: 40))
        XCTAssertTrue(element("Wind", in: app).exists)
        sleep(3)
        keepScreenshot("fly-3-glide")

        // The cockpit shows the way back to the take-off until the pilot
        // chooses a place; the waypoints are listed nearest first.
        let strip = element("target-strip", in: app)
        XCTAssertTrue(valued(containing: "Take-off", in: app).exists)
        strip.tap()
        XCTAssertTrue(app.navigationBars["Fly to"].waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("fly-4-fly-to")
        labelled(containing: "Sample landing field", in: app).tap()
        XCTAssertTrue(valued(containing: "Sample landing field", in: app).waitForExistence(timeout: 10))
        sleep(2)
        keepScreenshot("fly-5-to-landing")

        // The wind of every band of height the glider circled in, from a
        // tap on the wind over the map.
        element("wind-badge", in: app).tap()
        XCTAssertTrue(app.navigationBars["Wind by height"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("wind-by-height-row", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("fly-5b-wind-by-height")
        app.buttons["Done"].tap()

        app.buttons["End replay"].tap()
        let close = app.buttons["Close the cockpit"]
        XCTAssertTrue(close.waitForExistence(timeout: 10))
        close.tap()
        XCTAssertTrue(app.buttons["Start flight"].waitForExistence(timeout: 15))

        // Settings, and the credits the map data's licences ask for.
        openTab("Settings", in: app)
        XCTAssertTrue(app.staticTexts["Pilot"].waitForExistence(timeout: 15) || element("Vario sound", in: app).exists)
        keepScreenshot("settings-1")

        // The pilot's waypoints.
        app.buttons["Waypoints"].tap()
        XCTAssertTrue(labelled(containing: "Sample turnpoint", in: app).waitForExistence(timeout: 15))
        keepScreenshot("settings-3-waypoints")
        app.navigationBars.buttons.firstMatch.tap()

        let licences = app.buttons["Data and licences"]
        for _ in 0..<4 where !(licences.exists && licences.isHittable) {
            app.swipeUp()
        }
        app.buttons["Data and licences"].tap()
        XCTAssertTrue(app.staticTexts["Base map"].waitForExistence(timeout: 15))
        keepScreenshot("settings-2-credits")
    }

    /// Airspace: a made-up control zone lies beside the end of the sample
    /// flight. Replayed at twenty times its speed, the glider comes near it
    /// after about a minute, and the cockpit says so.
    func testTheCockpitWarnsOfAirspace() {
        let app = launch(bundledMap: true, extra: ["--sample-airspace"])

        // The zone is on the map tab.
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))

        openTab("Fly", in: app)
        app.buttons["Replay the sample flight"].tap()
        app.buttons["20 times faster"].tap()
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 20))

        // A minute of flying later the zone is still far off, and the
        // cockpit is checking: no notice, no warning.
        sleep(3)
        keepScreenshot("fly-6a-before-the-zone")
        XCTAssertTrue(app.buttons["End replay"].exists, "The cockpit can be read while it replays.")
        XCTAssertFalse(element("airspace-notice", in: app).exists)

        // The warning comes when the glider is two kilometres from the
        // zone, nine minutes into the flight.
        let banner = element("airspace-banner", in: app)
        XCTAssertTrue(banner.waitForExistence(timeout: 120))
        XCTAssertTrue(valued(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        sleep(2)
        keepScreenshot("fly-6-airspace")

        banner.tap()
        XCTAssertTrue(app.buttons["Seen: quiet for 10 minutes"].waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "Class D control zone", in: app).exists)
        sleep(1)
        keepScreenshot("fly-7-airspace-warnings")
        // "Seen" would speak again as the glider comes closer; "does not
        // apply today" stays quiet, which is what this test can rely on.
        app.buttons["Does not apply today"].tap()
        XCTAssertTrue(element("Airspace warning, quieted", in: app).waitForExistence(timeout: 10))

        app.buttons["End replay"].tap()
        let close = app.buttons["Close the cockpit"]
        XCTAssertTrue(close.waitForExistence(timeout: 10))
        close.tap()

        // The files behind the warnings.
        openTab("Settings", in: app)
        let files = app.buttons["Airspace"]
        XCTAssertTrue(files.waitForExistence(timeout: 15))
        files.tap()
        XCTAssertTrue(labelled(containing: "sample-zones-not-real", in: app).waitForExistence(timeout: 15))
        keepScreenshot("settings-4-airspace")

        // And the zone on the map: with the sample flight in the logbook
        // the map shows the valley the flight ends in, beside the zone.
        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        sleep(4)
        keepScreenshot("map-airspace-flight")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))
        openTab("Map", in: app)
        sleep(5)
        keepScreenshot("map-airspace")
    }

    /// The assistant, with the provider played by a script: the real tool
    /// loop runs against the logbook, but no key and no network are used.
    func testTheAssistantAnswersFromTheLogbook() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--bundled-map", "--scripted-assistant"]
        app.launch()

        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        // A flight covers the tab bar; back to the logbook first.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))

        openTab("Assistant", in: app)
        let question = app.buttons["How was my flying this year?"]
        if !question.waitForExistence(timeout: 8) {
            // The first tap can still be swallowed by the bar's animation.
            openTab("Assistant", in: app)
        }
        XCTAssertTrue(question.waitForExistence(timeout: 15))
        keepScreenshot("assistant-1-start")
        question.tap()

        XCTAssertTrue(labelled(containing: "Your logbook has one flight", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "Looked up your list of flights", in: app).exists)
        sleep(1)
        keepScreenshot("assistant-2-answer")
    }

    /// Live wind, from a Venti server running on the same machine in its
    /// development mode: six stations as they were on one evening. The CI
    /// workflow starts the server and names it; without one the test is
    /// skipped. On a Mac: `cargo run -p venti-server -- dev`, then run the
    /// tests with `TEST_RUNNER_VENTI_SERVER=http://127.0.0.1:8080`.
    func testTheMapShowsLiveWindFromAServer() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        // The model provider is played by a script; the Venti server is real.
        app.launchArguments = ["--ui-testing", "--server", server, "--scripted-assistant"]
        app.launch()

        let stations = app.buttons["Wind stations"]
        XCTAssertTrue(stations.waitForExistence(timeout: 90))
        // The credit appears with the first answer from the server.
        XCTAssertTrue(element("wind-credits", in: app).waitForExistence(timeout: 60))
        sleep(12)
        keepScreenshot("wind-1-map")

        stations.tap()
        XCTAssertTrue(app.navigationBars["Wind stations"].waitForExistence(timeout: 15))
        let summit = labelled(containing: "Zugspitze", in: app)
        XCTAssertTrue(summit.waitForExistence(timeout: 15))
        // A name the provider writes in capitals is shown in ordinary case.
        XCTAssertTrue(labelled(containing: "Patscherkofel", in: app).exists)
        sleep(1)
        keepScreenshot("wind-2-stations")

        summit.tap()
        XCTAssertTrue(element("wind-chart", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Deutscher Wetterdienst", in: app).exists)
        sleep(2)
        keepScreenshot("wind-3-station")

        // The assistant looks up the same wind on the same server.
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Done"].tap()
        openTab("Assistant", in: app)
        let question = app.buttons["How is the wind on the Zugspitze right now?"]
        XCTAssertTrue(question.waitForExistence(timeout: 15))
        question.tap()
        XCTAssertTrue(labelled(containing: "Zugspitze: 54 km/h", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "the wind stations around a place", in: app).exists)
        sleep(1)
        keepScreenshot("wind-4-assistant")
    }

    /// Airspace from the pilot's server: with a server set, the app has
    /// fetched the airspace around the map by itself. It says before
    /// take-off what the cockpit will warn from, and lists it with its
    /// provider's credit and notice. Skipped without a server.
    func testAirspaceComesFromTheServerByItself() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))

        // The Fly tab says what the cockpit will warn from.
        openTab("Fly", in: app)
        let line = element("airspace-before-takeoff", in: app)
        XCTAssertTrue(line.waitForExistence(timeout: 30))
        XCTAssertTrue(saying("openAIP", in: app).waitForExistence(timeout: 60))
        keepScreenshot("airspace-1-before-takeoff")

        line.tap()
        XCTAssertTrue(element("airspace-from-server", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(saying("not certified", in: app).exists)
        keepScreenshot("airspace-2-from-server")
    }

    /// An account on a Venti server: signing in as the development
    /// server's demo pilot brings that pilot's three flights into the
    /// logbook, and one of them is shared with a link. Skipped without a
    /// server, like the wind test.
    func testAPilotSignsInAndSharesAFlight() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))

        openTab("Settings", in: app)
        // The row is below the first screenful, and a row that is not on
        // screen does not exist yet.
        let link = element("account-link", in: app)
        for _ in 0..<4 where !(link.exists && link.isHittable) {
            app.swipeUp()
        }
        if !link.waitForExistence(timeout: 10) {
            keepScreenshot("account-0-settings-without-the-link")
        }
        XCTAssertTrue(link.exists)
        link.tap()
        let handle = app.textFields["Handle"]
        XCTAssertTrue(handle.waitForExistence(timeout: 15))
        keepScreenshot("account-1-sign-in")
        handle.tap()
        handle.typeText("demo")
        let password = app.secureTextFields["Password"]
        password.tap()
        password.typeText("demo-pilot-password")
        element("account-submit", in: app).tap()

        // Signed in, and the sync that follows has fetched the flights.
        XCTAssertTrue(labelled(containing: "@demo", in: app).waitForExistence(timeout: 30))
        // The iPhone offers to keep the password. Not in a test.
        for asking in [app, XCUIApplication(bundleIdentifier: "com.apple.springboard")] {
            let notNow = asking.buttons["Not Now"]
            if notNow.waitForExistence(timeout: 3) {
                notNow.tap()
                break
            }
        }
        XCTAssertTrue(app.buttons["Sync now"].waitForExistence(timeout: 60))
        sleep(2)
        keepScreenshot("account-2-signed-in")

        openTab("Logbook", in: app)
        let row = element("logbook-row", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("account-3-logbook")

        // Share the newest flight with those who have its link. Whatever
        // the demo pilot had chosen for it before, that is the choice now.
        row.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        app.buttons["More"].tap()
        app.buttons["Share with a link"].tap()
        let withTheLink = element("share-unlisted", in: app)
        XCTAssertTrue(withTheLink.waitForExistence(timeout: 15))
        withTheLink.tap()
        let chosen = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'share-unlisted' AND value CONTAINS 'Chosen'"))
            .firstMatch
        XCTAssertTrue(chosen.waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "/f/", in: app).exists)
        sleep(1)
        keepScreenshot("account-4-share")
    }

    /// The same screens with tiles fetched from the public servers, as a
    /// pilot's phone draws them. Only the screenshots matter here: the test
    /// does not fail when the network is slow.
    func testTheMapWithTilesFromTheNetwork() {
        let app = launch(bundledMap: false)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        sleep(12)
        keepScreenshot("online-1-map")

        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        sleep(12)
        keepScreenshot("online-2-flight")
    }
}
