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

    /// Anything on screen whose accessibility value contains this text.
    private func valued(containing text: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "value CONTAINS %@", text))
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

        app.tabBars.buttons["Logbook"].tap()
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
        app.tabBars.buttons["Map"].tap()
        sleep(4)
        keepScreenshot("light-7-map-with-flight")
        app.tabBars.buttons["Logbook"].tap()

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
        app.tabBars.buttons["Fly"].tap()
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

        app.buttons["End replay"].tap()
        let close = app.buttons["Close the cockpit"]
        XCTAssertTrue(close.waitForExistence(timeout: 10))
        close.tap()
        XCTAssertTrue(app.buttons["Start flight"].waitForExistence(timeout: 15))

        // Settings, and the credits the map data's licences ask for.
        app.tabBars.buttons["Settings"].tap()
        XCTAssertTrue(app.staticTexts["Pilot"].waitForExistence(timeout: 15) || element("Vario sound", in: app).exists)
        keepScreenshot("settings-1")

        // The pilot's waypoints.
        app.buttons["Waypoints"].tap()
        XCTAssertTrue(labelled(containing: "Sample turnpoint", in: app).waitForExistence(timeout: 15))
        keepScreenshot("settings-3-waypoints")
        app.navigationBars.buttons.firstMatch.tap()

        for _ in 0..<3 where !app.buttons["Data and licences"].isHittable {
            app.swipeUp()
        }
        app.buttons["Data and licences"].tap()
        XCTAssertTrue(app.staticTexts["Base map"].waitForExistence(timeout: 15))
        keepScreenshot("settings-2-credits")
    }

    /// The assistant, with the provider played by a script: the real tool
    /// loop runs against the logbook, but no key and no network are used.
    func testTheAssistantAnswersFromTheLogbook() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--bundled-map", "--scripted-assistant"]
        app.launch()

        app.tabBars.buttons["Logbook"].tap()
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        // A flight covers the tab bar; back to the logbook first.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))

        app.tabBars.buttons["Assistant"].tap()
        let question = app.buttons["How was my flying this year?"]
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
        app.tabBars.buttons["Assistant"].tap()
        let question = app.buttons["How is the wind on the Zugspitze right now?"]
        XCTAssertTrue(question.waitForExistence(timeout: 15))
        question.tap()
        XCTAssertTrue(labelled(containing: "Zugspitze: 54 km/h", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "the wind stations around a place", in: app).exists)
        sleep(1)
        keepScreenshot("wind-4-assistant")
    }

    /// The same screens with tiles fetched from the public servers, as a
    /// pilot's phone draws them. Only the screenshots matter here: the test
    /// does not fail when the network is slow.
    func testTheMapWithTilesFromTheNetwork() {
        let app = launch(bundledMap: false)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        sleep(12)
        keepScreenshot("online-1-map")

        app.tabBars.buttons["Logbook"].tap()
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        sleep(12)
        keepScreenshot("online-2-flight")
    }
}
