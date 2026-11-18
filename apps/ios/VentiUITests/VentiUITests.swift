import XCTest

/// Drives the app the way a pilot would, on a simulator, and keeps a
/// screenshot of every screen. The CI workflow publishes the screenshots, so
/// a change to the interface can be looked at without a Mac.
///
/// The app is started with an empty logbook in a throwaway folder.
@MainActor
final class VentiUITests: XCTestCase {
    private func launch(bundledMap: Bool, dark: Bool = false) -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing"] + (bundledMap ? ["--bundled-map"] : []) + (dark ? ["--dark"] : [])
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
        app.buttons["Done"].tap()

        // Back to the logbook, where the flight is now listed.
        app.navigationBars.buttons.firstMatch.tap()
        let row = element("logbook-row", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("light-6-logbook")

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
