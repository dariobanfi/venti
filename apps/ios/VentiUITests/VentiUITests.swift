import CoreLocation
import XCTest

/// Drives the app the way a pilot would, on a simulator, and keeps a
/// screenshot of every screen. The CI workflow publishes the screenshots, so
/// a change to the interface can be looked at without a Mac.
///
/// The app is started with an empty logbook in a throwaway folder.
@MainActor
final class VentiUITests: XCTestCase {
    /// A Venti that is not reached: nothing listens on this port. The app
    /// talks to the Venti it was built with unless a test names another,
    /// and a test that needs none names this one, so that it never asks a
    /// Venti that happens to run on the Mac.
    static let nowhere = "http://127.0.0.1:9"

    /// The arguments with a server named, `nowhere` where none is.
    private static func naming(_ arguments: [String]) -> [String] {
        arguments.contains("--server") ? arguments : arguments + ["--server", nowhere]
    }

    private func launch(bundledMap: Bool, dark: Bool = false, extra: [String] = []) -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = Self.naming(
            ["--ui-testing"] + (bundledMap ? ["--bundled-map"] : []) + (dark ? ["--dark"] : []) + extra
        )
        app.launch()
        // The app opens on Today; most tests begin on the map.
        openTab("Map", in: app)
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

    /// Whether an element lies wholly on the screen. `isHittable` cannot
    /// be asked of an element that was scrolled out of a row: XCTest
    /// fails the test with "Failed to determine hittability".
    private func isOnScreen(_ element: XCUIElement, in app: XCUIApplication) -> Bool {
        guard element.exists else { return false }
        let frame = element.frame
        return !frame.isEmpty && app.windows.firstMatch.frame.contains(frame)
    }

    /// The layers the map tab keeps as switches in its layers sheet; the
    /// others ("layer-wind", "layer-sites", the forecast and the radar)
    /// are chips on the map, and airspace ("layer-airspace") is its own
    /// round button too, which `setLayer` taps (`setAirspace`).
    private static let layersInTheSheet: Set<String> = [
        "layer-pilots", "layer-cables", "map-aerial", "layer-climbs", "layer-flown",
    ]

    /// Switches airspace on the map tab on or off with its round button,
    /// one tap: off until the pilot switches it on, and the choice is kept
    /// between launches, so a test looks which way it stands first.
    private func setAirspace(on: Bool, in app: XCUIApplication) {
        let button = app.buttons["map-airspace"]
        guard button.waitForExistence(timeout: 30) else {
            XCTFail("The map has no airspace button.")
            return
        }
        if button.isSelected != on {
            button.tap()
            usleep(700_000)
        }
        XCTAssertEqual(button.isSelected, on)
    }

    /// Opens the map tab's layers sheet, from the first of its two round
    /// buttons: the layers without a chip are switches in it, and the
    /// lists of what the map shows are rows. Nothing happens where it is
    /// open already.
    @discardableResult
    private func openLayers(in app: XCUIApplication) -> Bool {
        if app.navigationBars["Layers"].exists {
            return true
        }
        let layers = app.buttons["map-layers"]
        guard layers.waitForExistence(timeout: 30) else { return false }
        layers.tap()
        return app.navigationBars["Layers"].waitForExistence(timeout: 10)
    }

    /// Closes the layers sheet, where it is open.
    private func closeLayers(in app: XCUIApplication) {
        let done = app.navigationBars["Layers"].buttons["Done"]
        if done.exists {
            done.tap()
            usleep(700_000)
        }
    }

    /// A row of the layers sheet, by its name or identifier: "Wind
    /// stations", "Flying sites", "sites-filter", "Pilots in the air",
    /// "Flying plans". Opens the sheet first and waits for the row, which
    /// is there only once the map has what it lists. A tap on it closes
    /// the sheet and opens the list.
    private func layersRow(_ name: String, in app: XCUIApplication, timeout: TimeInterval = 60) -> XCUIElement {
        XCTAssertTrue(openLayers(in: app), "The layers sheet opens from the map.")
        let row = app.buttons[name]
        _ = row.waitForExistence(timeout: timeout)
        return row
    }

    /// Waits until the map's credits, behind its credits button, name
    /// something: "Wind", "Sites". The credit comes with the first answer
    /// from the server, so this is how a test knows the map has it.
    @discardableResult
    private func waitForCredit(_ what: String, in app: XCUIApplication, timeout: TimeInterval = 60) -> Bool {
        let credits = app.buttons["map-credits"]
        guard credits.waitForExistence(timeout: timeout) else { return false }
        // A layer's credit is there only while the layer is: the chips'
        // choices are kept between launches, and a test before this one
        // may have left one off.
        if let chip = ["Wind": "layer-wind", "Sites": "layer-sites"][what] {
            setLayer(chip, on: true, in: app)
        }
        let named = NSPredicate(format: "value CONTAINS %@", what)
        return XCTWaiter().wait(for: [XCTNSPredicateExpectation(predicate: named, object: credits)], timeout: timeout) == .completed
    }

    /// Switches one of the map tab's layers on or off: a chip on the map,
    /// or a switch in the layers sheet, which is opened for it and closed
    /// again.
    private func setLayer(_ identifier: String, on: Bool, in app: XCUIApplication) {
        if identifier == "layer-airspace" {
            setAirspace(on: on, in: app)
            return
        }
        if Self.layersInTheSheet.contains(identifier) {
            guard openLayers(in: app) else { return }
            let toggle = app.switches[identifier]
            if toggle.waitForExistence(timeout: 5) {
                set(toggle, on: on)
            }
            closeLayers(in: app)
            return
        }
        let chip = element(identifier, in: app)
        guard chip.waitForExistence(timeout: 10) else { return }
        // The row scrolls where the text is large: a chip off the screen
        // is brought into it first, by swiping the row on one that is on it.
        for direction in [true, true, false, false] where !isOnScreen(chip, in: app) {
            let handle = ["layer-wind", "layer-sites", "forecast-chip"]
                .map { element($0, in: app) }
                .first { isOnScreen($0, in: app) }
            if direction {
                handle?.swipeLeft()
            } else {
                handle?.swipeRight()
            }
            usleep(600_000)
        }
        if isOnScreen(chip, in: app), chip.isSelected != on {
            chip.tap()
            usleep(700_000)
        }
    }

    /// Turns the cockpit to its next page, with a tap on the dots in its
    /// top line: from the readings to the map, to the flight with what can
    /// be switched, and around.
    private func nextPage(in app: XCUIApplication) {
        let dots = element("cockpit-pages", in: app)
        XCTAssertTrue(dots.waitForExistence(timeout: 15))
        dots.tap()
        usleep(500_000)
    }

    /// Turns the cockpit's page as a pilot does in flight: a swipe across
    /// the middle of the screen, which on every page is the map. To the
    /// left the next page, to the right the one before. Says which page is
    /// then in front, as the dots say it.
    @discardableResult
    private func swipePage(toNext: Bool, in app: XCUIApplication) -> String {
        let dots = element("cockpit-pages", in: app)
        XCTAssertTrue(dots.waitForExistence(timeout: 15))
        let before = dots.value as? String
        if toNext {
            app.swipeLeft()
        } else {
            app.swipeRight()
        }
        usleep(500_000)
        let after = dots.value as? String ?? ""
        XCTAssertNotEqual(after, before, "A swipe sideways turns the page.")
        return after
    }

    /// Ends the session in the cockpit: "End" in its top line is held, and
    /// nothing asks again.
    private func endTheCockpit(in app: XCUIApplication) {
        let end = element("cockpit-end", in: app)
        XCTAssertTrue(end.waitForExistence(timeout: 10))
        end.press(forDuration: 1.6)
    }

    /// Half a metre to and fro, for `put`.
    private var wander = 0.0

    /// Puts the simulated iPhone at a moment of a made-up flight: so many
    /// metres north of a take-off above the Tegernsee, at a height and a
    /// speed. A receiver's positions wander while it stands, and the
    /// simulator hands a position on only when it is another than the
    /// last: so these wander too, by half a metre.
    /// The height the map that comes with the app has for the ground at
    /// that take-off (47.664 N, 11.772 E), metres: the cockpit showed
    /// "533 m over ground" at 1600 m there on 2026-11-22. To look at again
    /// when `tools/demo-map/build.sh` builds the region anew.
    private static let takeoffGroundM: Double = 1067

    /// The cockpit's "… m over ground" under the altitude, in metres;
    /// `nil` while it shows none.
    private func overGround(in app: XCUIApplication) -> Double? {
        let reading = saying(" m over ground", in: app)
        guard reading.waitForExistence(timeout: 20) else { return nil }
        let said = "\(reading.label) \(reading.value.map { "\($0)" } ?? "")"
        guard let found = said.firstMatch(of: /(\d+) m over ground/) else { return nil }
        return Double(found.1)
    }

    private func put(metresNorth: Double, altitude: Double, speed: Double) {
        wander = wander == 0 ? 0.5 : 0
        let north = metresNorth + wander
        XCUIDevice.shared.location = XCUILocation(
            location: CLLocation(
                coordinate: CLLocationCoordinate2D(latitude: 47.664 + north / 111_195, longitude: 11.772),
                altitude: altitude, horizontalAccuracy: 5, verticalAccuracy: 5,
                course: 0, speed: speed, timestamp: Date()
            )
        )
    }

    /// Waits until everything named, by identifier or by label, is on the
    /// screen at one and the same look, and says whether it came to that.
    /// What a replay shows passes: looked for one thing after the other,
    /// the first can be gone again before the last is found, the more so
    /// on a slow machine, where one look at the screen takes seconds.
    private func waitForAll(_ names: Set<String>, in app: XCUIApplication, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if let look = try? app.snapshot() {
                var missing = names
                var open: [any XCUIElementSnapshot] = [look]
                while let next = open.popLast(), !missing.isEmpty {
                    missing.remove(next.identifier)
                    missing.remove(next.label)
                    open.append(contentsOf: next.children)
                }
                if missing.isEmpty {
                    return true
                }
            }
            Thread.sleep(forTimeInterval: 0.5)
        } while Date() < deadline
        return false
    }

    /// Taps something that may still be moving into place, such as an item
    /// of a menu that is unfolding: a tap that comes too early touches the
    /// menu and chooses nothing.
    private func tapWhenStill(_ element: XCUIElement, timeout: TimeInterval = 10) {
        XCTAssertTrue(element.waitForExistence(timeout: timeout))
        var last = element.frame
        for _ in 0..<40 {
            usleep(200_000)
            let now = element.frame
            if now == last, element.isHittable {
                break
            }
            last = now
        }
        element.tap()
    }

    /// Chooses an item of a menu, and once more if the menu is still open
    /// afterwards with nothing chosen.
    private func choose(_ item: String, from opener: XCUIElement, in app: XCUIApplication) {
        XCTAssertTrue(opener.waitForExistence(timeout: 15))
        opener.tap()
        let choice = app.buttons[item]
        for _ in 0..<3 {
            tapWhenStill(choice)
            // Chosen: the menu has closed and its items are gone.
            usleep(700_000)
            if !choice.exists {
                return
            }
        }
        XCTFail("The menu did not take \"\(item)\"")
    }

    /// Changes to a tab. After a screen that covered the tab bar, and just
    /// after launch, the bar is still settling and a tap goes nowhere: wait
    /// until the button can be hit, tap, and tap again if the tab did not
    /// become the selected one.
    private func openTab(_ name: String, in app: XCUIApplication) {
        // The settings and the assistant are pages of the Today tab,
        // opened from its corner. A second tap on the tab goes back to
        // Today itself, whatever page lay over it.
        if name == "Settings" || name == "Assistant" {
            openTab("Today", in: app)
            app.tabBars.buttons["Today"].tap()
            usleep(600_000)
            let opener = app.navigationBars.buttons[name]
            XCTAssertTrue(opener.waitForExistence(timeout: 15))
            opener.tap()
            XCTAssertTrue(app.navigationBars[name].waitForExistence(timeout: 15))
            return
        }
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
        // The moment's numbers: with the ground under the flight, which
        // the region inside the app has, the height over it; until that
        // is read, the time in the air.
        XCTAssertTrue(element("Over ground", in: app).exists || element("In the air", in: app).exists)
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
        // The moment's numbers: with the ground under the flight, which
        // the region inside the app has, the height over it; until that
        // is read, the time in the air.
        XCTAssertTrue(element("Over ground", in: app).exists || element("In the air", in: app).exists)
        app.buttons["Show flight totals"].tap()

        // Back to the logbook, where the flight is now listed.
        app.navigationBars.buttons.firstMatch.tap()
        let row = element("logbook-row", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("light-6-logbook")

        // The totals lead to the statistics.
        element("logbook-totals", in: app).tap()
        XCTAssertTrue(app.staticTexts["Last twelve months"].waitForExistence(timeout: 15))
        XCTAssertTrue(element("statistics-year-2026", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("light-6b-statistics")
        // Further down: the months of the year, the typical flight, the
        // kinds of flight, the gliders and the bests. The list makes its
        // rows as they come into view.
        app.swipeUp()
        XCTAssertTrue(app.staticTexts["A typical flight"].waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("light-6c-statistics-typical")
        // The places flown from: the sample's one, without a name. Kept
        // as a screenshot when it comes into view; not a condition.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
            .press(forDuration: 0.2, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4)))
        if element("statistics-by-take-off", in: app).waitForExistence(timeout: 5) {
            sleep(1)
            keepScreenshot("light-6e-statistics-by-take-off")
        }
        app.swipeUp()
        XCTAssertTrue(app.staticTexts["Your bests"].waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("light-6d-statistics-bests")
        // At the end, the way to how the pilot flies. The sample logbook
        // has too few thermals for an observation, so the screen says so
        // (with observations: `testHowYouFlySaysWhatTheTracksShow`). Kept
        // when it is reached; not a condition.
        app.swipeUp()
        let howYouFly = element("statistics-how-you-fly", in: app)
        if howYouFly.waitForExistence(timeout: 5), howYouFly.isHittable {
            howYouFly.tap()
            let said = app.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier IN %@", ["how-you-fly-empty", "how-you-fly-card"]))
                .firstMatch
            if said.waitForExistence(timeout: 30) {
                sleep(1)
                keepScreenshot("light-6f-how-you-fly-empty")
            }
            if app.navigationBars["How you fly"].exists {
                app.navigationBars.buttons.firstMatch.tap()
                _ = app.staticTexts["Your bests"].waitForExistence(timeout: 10)
            }
        }
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

    /// "How you fly" with something to say: the logbook starts with the
    /// sample flight on five days (`--sample-logbook`), ten thermals, and
    /// the screen at the foot of the statistics shows its observations.
    func testHowYouFlySaysWhatTheTracksShow() {
        let app = launch(bundledMap: true, extra: ["--sample-logbook"])
        openTab("Logbook", in: app)
        let totals = element("logbook-totals", in: app)
        XCTAssertTrue(totals.waitForExistence(timeout: 30))
        totals.tap()
        XCTAssertTrue(app.staticTexts["Last twelve months"].waitForExistence(timeout: 15))

        // The way there is at the end of the statistics, whose list makes
        // its rows as they come into view.
        let howYouFly = element("statistics-how-you-fly", in: app)
        for _ in 0..<10 where !isOnScreen(howYouFly, in: app) {
            app.swipeUp()
            usleep(500_000)
        }
        XCTAssertTrue(isOnScreen(howYouFly, in: app))
        howYouFly.tap()
        XCTAssertTrue(app.navigationBars["How you fly"].waitForExistence(timeout: 15))
        // The core counts each flight from its file the first time.
        XCTAssertTrue(element("how-you-fly-card", in: app).waitForExistence(timeout: 60))
        XCTAssertFalse(element("how-you-fly-empty", in: app).exists)
        sleep(1)
        keepScreenshot("light-6f-how-you-fly")
    }

    /// A flight of the logbook on its way to a league: what the pilot is
    /// told first, the file, XContest's page and the DHV-XC's form.
    /// Nothing is sent: the test signs in nowhere and never taps "Send".
    func testAFlightIsHandedToALeague() {
        let app = launch(bundledMap: true)
        addSampleFlightAndOpenIt(in: app, prefix: "league")

        // The flight's own button, under its profile.
        tapWhenStill(element("send-to-league", in: app))
        XCTAssertTrue(app.navigationBars["Send to a league"].waitForExistence(timeout: 15))

        // The sample flight is of 4 July 2026: the leagues take it until
        // the 18th, whether that is past or to come.
        XCTAssertTrue(element("league-deadline", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "18 Jul 2026", in: app).exists)
        // Venti's own recorder made it, and signs nothing yet.
        XCTAssertTrue(labelled(containing: "does not sign its files yet", in: app).exists)

        // XContest: the file for the pilot, and the page to upload it on.
        XCTAssertTrue(element("league-file", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(element("xcontest-claim", in: app).exists)
        sleep(1)
        keepScreenshot("league-4-sheet")

        // The DHV-XC: nothing can be sent without a name and the upload
        // password.
        let send = element("dhvxc-send", in: app)
        for _ in 0..<4 where !send.isHittable {
            app.swipeUp()
        }
        XCTAssertTrue(send.waitForExistence(timeout: 10))
        XCTAssertFalse(send.isEnabled)
        sleep(1)
        keepScreenshot("league-5-dhvxc")

        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 15))
    }

    func testTheAppInDarkAppearance() {
        let app = launch(bundledMap: true, dark: true)
        addSampleFlightAndOpenIt(in: app, prefix: "dark")

        // A flight of the logbook once more, as the cockpit would have
        // shown it in the air.
        app.buttons["More"].tap()
        // A menu within the menu: its speeds unfold where it stood.
        tapWhenStill(app.buttons["Replay in the cockpit"])
        tapWhenStill(app.buttons["20 times faster"])
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 30))
        // The thermal's numbers and its picture, at one look: the thermal
        // is over a quarter of a minute after it began.
        XCTAssertTrue(waitForAll(["Gained", "lift-badge"], in: app, timeout: 180))
        sleep(1)
        keepScreenshot("dark-3b-replayed-in-the-cockpit")
        // A replay can be held at a moment, to look at it, and run on.
        choose("Paused", from: element("replay-speed", in: app), in: app)
        XCTAssertTrue(labelled(containing: "Replay paused", in: app).waitForExistence(timeout: 10))
        choose("10 times faster", from: element("replay-speed", in: app), in: app)
        XCTAssertTrue(labelled(containing: "Replay ×10", in: app).waitForExistence(timeout: 10))
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))
        // Back to the logbook, where the flight's page still is. It hides
        // the tab bar, so the tab cannot be asked whether it is selected.
        let logbookTab = app.tabBars.buttons["Logbook"]
        for _ in 0..<4 where !app.buttons["Flight details"].exists {
            for _ in 0..<40 where !(logbookTab.exists && logbookTab.isHittable) {
                usleep(250_000)
            }
            usleep(400_000)
            if logbookTab.exists {
                logbookTab.tap()
            }
            _ = app.buttons["Flight details"].waitForExistence(timeout: 5)
        }
        XCTAssertTrue(app.buttons["Flight details"].exists)

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
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 30))
        keepScreenshot("fly-1-ready")

        // Who may watch a flight while it is under way: nobody, until the
        // pilot says otherwise, and nobody can be chosen without an
        // account to send the positions to.
        element("live-before-takeoff", in: app).tap()
        XCTAssertTrue(app.navigationBars["Live tracking"].waitForExistence(timeout: 10))
        XCTAssertTrue(saying("Nothing is sent", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("fly-1b-live")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 10))

        choose("20 times faster", from: app.buttons["Replay the sample flight"], in: app)
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 20))

        // Circling in lift, the cockpit changes to its thermal numbers by
        // itself. Most of a circle later the map pictures the thermal: the
        // circles flown, moved with the wind, and where among them the
        // lift is. The sample waypoints hold one landing, well within
        // reach from up here: the cockpit names it with the height to
        // spare. All three at one look, because the thermal is over a
        // quarter of a minute after it began.
        XCTAssertTrue(waitForAll(["Gained", "lift-badge", "landing-badge"], in: app, timeout: 180))
        // The circles are pictured in the thermal view beside the map.
        XCTAssertTrue(element("thermal-view", in: app).exists)
        sleep(1)
        keepScreenshot("fly-2-thermal")

        // On the glide after the thermal it changes back, now with the wind
        // it worked out while circling.
        XCTAssertTrue(element("Glide", in: app).waitForExistence(timeout: 150))
        XCTAssertTrue(element("Wind", in: app).exists)
        sleep(3)
        // On a glide the map has the room: the thermal view is there
        // only while the core says the glider circles.
        XCTAssertFalse(element("thermal-view", in: app).exists)
        keepScreenshot("fly-3-glide")

        // The second page: the map as large as it gets, and under it the
        // way to the place flown to. The cockpit shows the way back to the
        // take-off until the pilot chooses a place; the waypoints are
        // listed nearest first. Pages are turned by a swipe: there and back
        // and there again.
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Map"))
        XCTAssertTrue(swipePage(toNext: false, in: app).hasPrefix("Instruments"))
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Map"))
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
        // Over the bands, the wind the cockpit shows in the core's
        // words: how strong, where from, from what and how old.
        XCTAssertTrue(element("wind-words", in: app).exists)
        sleep(1)
        keepScreenshot("fly-5b-wind-by-height")
        app.buttons["Done"].tap()

        // The whole flight at a glance: the scored route, and the two
        // areas in which a turnpoint would make an FAI triangle of the
        // longest line flown.
        app.buttons["Show the whole flight"].tap()
        XCTAssertTrue(app.buttons["Follow the glider"].waitForExistence(timeout: 10))
        sleep(3)
        keepScreenshot("fly-5c-whole-flight")
        app.buttons["Follow the glider"].tap()
        XCTAssertTrue(app.buttons["Show the whole flight"].waitForExistence(timeout: 10))

        // The aerial view, at the layers button over the map. The
        // photographs need the network and may not come in CI: what is
        // kept is a screenshot of the button switched on, and the choice,
        // which every map shares, is switched off again. Not a condition.
        let aerial = element("cockpit-aerial", in: app)
        if aerial.waitForExistence(timeout: 5) {
            if aerial.isSelected {
                aerial.tap()
                usleep(700_000)
            }
            aerial.tap()
            usleep(700_000)
            if aerial.isSelected {
                sleep(4)
                keepScreenshot("cockpit-aerial")
                aerial.tap()
                usleep(700_000)
            }
        }

        // The third page: how the flight stands, and what can be switched.
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Flight"))
        let lock = element("cockpit-lock", in: app)
        XCTAssertTrue(lock.waitForExistence(timeout: 10))
        XCTAssertTrue(element("Flight time", in: app).exists)
        XCTAssertTrue(element("cockpit-mute", in: app).exists)
        sleep(1)
        keepScreenshot("fly-5d-flight-page")

        // What the cockpit knows beyond its pages' readings, from the
        // third page: each a row in the core's words.
        element("cockpit-details", in: app).tap()
        XCTAssertTrue(app.navigationBars["Details"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("cockpit-detail", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("fly-5d2-details")
        app.navigationBars["Details"].buttons["Done"].tap()
        XCTAssertTrue(lock.waitForExistence(timeout: 10))

        // Locked, the screen answers no touch: neither a tap on the dots
        // nor a swipe turns the page, and "End" ends nothing. Holding the
        // lock opens it.
        lock.tap()
        let unlock = element("cockpit-unlock", in: app)
        XCTAssertTrue(unlock.waitForExistence(timeout: 10))
        element("cockpit-pages", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        app.swipeLeft()
        element("cockpit-end", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 1.6)
        sleep(1)
        XCTAssertTrue(lock.exists)
        XCTAssertTrue((element("cockpit-pages", in: app).value as? String)?.hasPrefix("Flight") == true)
        keepScreenshot("fly-5e-locked")
        unlock.press(forDuration: 1.6)
        sleep(1)
        XCTAssertFalse(element("cockpit-unlock", in: app).exists)
        XCTAssertTrue(lock.exists)

        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))

        // Settings, and the credits the map data's licences ask for.
        openTab("Settings", in: app)
        XCTAssertTrue(app.staticTexts["Pilot"].waitForExistence(timeout: 15) || element("Vario sound", in: app).exists)
        keepScreenshot("settings-1")

        // The glider, the FAI triangle areas and the brightness, on a
        // page of their own: no class and no areas until the pilot says.
        let cockpit = element("cockpit-settings-link", in: app)
        for _ in 0..<3 where !(cockpit.exists && cockpit.isHittable) {
            app.swipeUp()
        }
        cockpit.tap()
        XCTAssertTrue(element("glider-class", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(isOn(app.switches["fai-areas"]) == false)
        sleep(1)
        keepScreenshot("settings-1b-cockpit")
        app.navigationBars.buttons.firstMatch.tap()
        for _ in 0..<3 where !app.buttons["Waypoints"].isHittable {
            app.swipeDown()
        }

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

    /// A flight that records itself, with positions the test makes up:
    /// the cockpit waits on the ground and writes nothing, takes off when
    /// the iPhone moves as a glider does, lands when it stops, and two
    /// minutes later the flight is in the logbook without a tap.
    func testAFlightRecordsItself() {
        let app = XCUIApplication()
        app.resetAuthorizationStatus(for: .location)
        app.launchArguments = Self.naming(["--ui-testing", "--bundled-map"])
        continueAfterFailure = false
        app.launch()
        openTab("Fly", in: app)
        let open = app.buttons["Open the cockpit"]
        XCTAssertTrue(open.waitForExistence(timeout: 30))

        // The flight stands, flies and lands at the height the app's own
        // terrain has for the take-off: the core ends no flight while
        // known ground lies more than 50 m under the glider.
        let ground = Self.takeoffGroundM
        put(metresNorth: 0, altitude: ground, speed: 0)
        open.tap()
        // The system asks whether Venti may use the position.
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow While Using App"]
        if allow.waitForExistence(timeout: 15) {
            allow.tap()
        }

        // Standing at the take-off: ready, and nothing is recorded.
        for _ in 0..<8 {
            put(metresNorth: 0, altitude: ground, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(saying("Ready for take-off", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(saying("Recording starts by itself", in: app).exists)
        XCTAssertTrue(element("Altitude", in: app).exists)
        // The made-up height is the ground's: the cockpit says so.
        XCTAssertLessThanOrEqual(
            overGround(in: app) ?? .infinity, 25, "The take-off of the made-up flight is not on the app's ground."
        )
        keepScreenshot("self-1-ready")
        // What can be switched before the take-off, with "Record now" for
        // a pilot who would rather not wait.
        nextPage(in: app)
        nextPage(in: app)
        XCTAssertTrue(element("cockpit-record-now", in: app).waitForExistence(timeout: 10))
        keepScreenshot("self-2-ready-flight-page")
        nextPage(in: app)

        // Ninety seconds of flight at the take-off's height, 10 m/s over
        // the ground: 450 m out over the valley, and back to where it
        // started. The take-off is noticed within seconds.
        for second in 1...90 {
            let out = Double(min(second, 90 - second))
            put(metresNorth: 10 * out, altitude: ground, speed: 10)
            sleep(1)
            if second == 20 {
                XCTAssertTrue(saying("Recording", in: app).waitForExistence(timeout: 10))
                XCTAssertFalse(saying("Ready for take-off", in: app).exists)
                keepScreenshot("self-3-in-the-air")
            }
            if second == 45 {
                // Where the flight of this test stood still until
                // 2026-11-22, at 1555 m: what lies under it here.
                keepScreenshot("self-3b-over-the-valley")
            }
        }

        // Standing where it took off, on the ground: landed after twenty
        // seconds, and the cockpit says when the flight will be saved.
        for _ in 0..<40 {
            put(metresNorth: 0, altitude: ground, speed: 0)
            sleep(1)
        }
        keepScreenshot("self-4a-standing-on-the-ground")
        XCTAssertTrue(saying("The flight is saved in", in: app).waitForExistence(timeout: 30))
        keepScreenshot("self-4-landed")

        // Two minutes after it came to rest the flight is in the logbook,
        // and its page is open.
        let details = app.buttons["Flight details"]
        for _ in 0..<150 where !details.exists {
            put(metresNorth: 0, altitude: ground, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(details.waitForExistence(timeout: 30))
        XCTAssertFalse(element("cockpit-end", in: app).exists)
        sleep(2)
        keepScreenshot("self-5-saved")
    }

    /// Airspace: a made-up control zone lies beside the end of the sample
    /// flight. Replayed at twenty times its speed, the glider comes near it
    /// after about a minute, and the cockpit says so.
    func testTheCockpitWarnsOfAirspace() {
        let app = launch(bundledMap: true, extra: ["--sample-airspace"])

        // The map tab's airspace is off while the cockpit flies: its
        // warnings must not depend on what that map draws. (The choice is
        // kept between launches, and a test before may have left it on.)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setAirspace(on: false, in: app)

        openTab("Fly", in: app)
        choose("20 times faster", from: app.buttons["Replay the sample flight"], in: app)
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 20))

        // A minute of flying later the zone is still far off, and the
        // cockpit is checking: no notice, no warning.
        sleep(3)
        keepScreenshot("fly-6a-before-the-zone")
        XCTAssertTrue(element("cockpit-end", in: app).exists, "The cockpit can be read while it replays.")
        XCTAssertFalse(element("airspace-notice", in: app).exists)

        // The airspace that matters next stands all the time, small over a
        // corner of the first page's map: the zone, already in reach from
        // the take-off, though no warning is up.
        let proximity = element("airspace-proximity", in: app)
        XCTAssertTrue(proximity.waitForExistence(timeout: 60))
        XCTAssertTrue(proximity.value.map { "\($0)" }?.contains("SAMPLE ZONE (NOT REAL)") == true)
        sleep(1)
        keepScreenshot("fly-8-airspace-proximity")

        // The warning comes when the glider is two kilometres from the
        // zone, nine minutes into the flight.
        let banner = element("airspace-banner", in: app)
        XCTAssertTrue(banner.waitForExistence(timeout: 300))
        XCTAssertTrue(valued(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        sleep(2)
        keepScreenshot("fly-6-airspace")

        // What lies on the line ahead, from the side: the zone as a block
        // from the ground up, the ground under it, and the glide running
        // into the one or the other. Looked at while the glider still
        // glides towards the zone: holding the replay first took until it
        // circled again. Its button is on the map's page.
        nextPage(in: app)
        app.buttons["Ahead, from the side"].tap()
        XCTAssertTrue(element("ahead-words", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE", in: app).exists)
        // The ground is in the picture: the region that comes with the
        // app has its elevation, and the glide ends on it.
        XCTAssertTrue(element("ahead-ground", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "Your line", in: app).exists)
        sleep(1)
        keepScreenshot("fly-6b-airspace-ahead")
        app.buttons["Done"].tap()

        // On the map's page the same reading is large, under the map, with
        // its height line; the small one is not there.
        XCTAssertTrue(element("airspace-proximity-map", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(valued(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        XCTAssertFalse(element("airspace-proximity", in: app).exists)
        sleep(1)
        keepScreenshot("fly-8b-airspace-proximity-map")

        // On a slow machine a tap can fall into the moment the band is
        // drawn anew, and is lost: once more, then.
        banner.tap()
        let seen = app.buttons["Seen: quiet for 10 minutes"]
        if !seen.waitForExistence(timeout: 6) {
            banner.tap()
        }
        XCTAssertTrue(seen.waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "Class D control zone", in: app).exists)
        sleep(1)
        keepScreenshot("fly-7-airspace-warnings")
        // "Seen" would speak again as the glider comes closer; "does not
        // apply today" stays quiet, which is what this test can rely on.
        app.buttons["Does not apply today"].tap()
        XCTAssertTrue(element("Airspace warning, quieted", in: app).waitForExistence(timeout: 10))

        endTheCockpit(in: app)

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

        // The flight's details say what it entered: the sample flight
        // clips the zone's edge in its second thermal.
        app.buttons["Flight details"].tap()
        let crossing = element("airspace-crossing-row", in: app)
        for _ in 0..<4 where !crossing.exists {
            app.swipeUp()
        }
        XCTAssertTrue(crossing.waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        sleep(1)
        keepScreenshot("flight-airspace")
        app.buttons["Done"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))
        openTab("Map", in: app)
        // Airspace is off on the map tab until the pilot switches it on.
        setAirspace(on: true, in: app)
        sleep(5)
        keepScreenshot("map-airspace")

        // A tap where the zone is drawn and nothing else lies: what is
        // over the place, as a sheet, with the filter by height at its
        // foot. The zone lies beside the flight; the map is tapped across
        // until the sheet comes.
        let over = element("airspace-over-words", in: app)
        search: for row in stride(from: 0.25, through: 0.7, by: 0.075) {
            for column in stride(from: 0.15, through: 0.85, by: 0.1) {
                app.coordinate(withNormalizedOffset: CGVector(dx: column, dy: row)).tap()
                if over.waitForExistence(timeout: 1.5) {
                    break search
                }
            }
        }
        XCTAssertTrue(over.exists, "No tap on the map opened what lies over a place.")
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        XCTAssertTrue(element("airspace-over-caveat", in: app).exists)
        sleep(1)
        keepScreenshot("airspace-over-1-place")
        // The foot of the sheet: the filter, in the core's words. Its
        // choice is kept between launches, so the test leaves it at all
        // heights.
        let filter = element("airspace-height-filter", in: app)
        for _ in 0..<4 where !isOnScreen(filter, in: app) {
            app.swipeUp()
        }
        XCTAssertTrue(filter.waitForExistence(timeout: 10))
        let lowest = filter.buttons.element(boundBy: filter.buttons.count - 1)
        lowest.tap()
        XCTAssertTrue(lowest.isSelected)
        sleep(1)
        keepScreenshot("airspace-over-2-filter")
        filter.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(filter.buttons.element(boundBy: 0).isSelected)
        app.buttons["Done"].tap()
        // Off again, as the map tab opens.
        setAirspace(on: false, in: app)
    }

    /// The map tab opens without airspace: not drawn, and a tap where the
    /// made-up zone lies opens nothing. The round button over the layers
    /// button switches it on with one tap (drawn, and the layers sheet's
    /// switch with it), and off again. Zoomed out too far, the map says
    /// to zoom in, in one line at most, and not at all while Venti is not
    /// reached. Light, then dark.
    func testTheMapOpensWithoutAirspaceAndOneTapShowsIt() throws {
        for dark in [false, true] {
            // The made-up zone is a circle of one nautical mile around
            // 47.70 N, 11.733 E: at the map's middle.
            let app = launch(bundledMap: true, dark: dark, extra: ["--sample-airspace", "--map-at", "47.7,11.7333,12"])
            let button = app.buttons["map-airspace"]
            XCTAssertTrue(button.waitForExistence(timeout: 90))
            // The choice is kept between launches: an earlier run that
            // stopped half-way may have left it on.
            setAirspace(on: false, in: app)
            XCTAssertEqual(button.label, "Airspace")
            sleep(3)
            keepScreenshot(dark ? "map-airspace-1-off-dark" : "map-airspace-1-off")

            // Off: a tap on the zone opens no sheet.
            let over = element("airspace-over-words", in: app)
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            XCTAssertFalse(over.waitForExistence(timeout: 3), "Airspace switched off opened a sheet.")

            // On with one tap: drawn, and a tap on it says what is over
            // the place.
            button.tap()
            usleep(700_000)
            XCTAssertTrue(button.isSelected)
            sleep(3)
            keepScreenshot(dark ? "map-airspace-2-on-dark" : "map-airspace-2-on")
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            XCTAssertTrue(over.waitForExistence(timeout: 10), "The zone at the map's middle is not drawn.")
            XCTAssertTrue(labelled(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
            app.buttons["Done"].tap()
            usleep(700_000)

            // The layers sheet's switch is the same choice.
            XCTAssertTrue(openLayers(in: app))
            let toggle = app.switches["layer-airspace"]
            XCTAssertTrue(toggle.waitForExistence(timeout: 5))
            XCTAssertTrue(isOn(toggle))
            set(toggle, on: false)
            closeLayers(in: app)
            XCTAssertFalse(button.isSelected, "The sheet's switch did not move the button.")
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            XCTAssertFalse(over.waitForExistence(timeout: 3))
            app.terminate()
        }

        // Too far out to draw, with Venti not reached: the wind's notice
        // says so, and no line asks to zoom in.
        let zoomIn = "zoom-in-notice"
        let away = launch(bundledMap: true, extra: ["--sample-airspace", "--map-at", "47.7,11.7333,5"])
        setAirspace(on: true, in: away)
        XCTAssertTrue(element("wind-notice", in: away).waitForExistence(timeout: 30))
        XCTAssertFalse(element(zoomIn, in: away).waitForExistence(timeout: 3))
        sleep(1)
        keepScreenshot("map-airspace-3-too-wide-not-reached")
        setAirspace(on: false, in: away)
        away.terminate()

        // With Venti: one line, airspace's while it is on, and another
        // layer's (the gliders of the other networks) once it is off.
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.7,11.7333,5"])
        setLayer("layer-pilots", on: true, in: app)
        setAirspace(on: true, in: app)
        let notice = element(zoomIn, in: app)
        XCTAssertTrue(notice.waitForExistence(timeout: 30))
        XCTAssertEqual(notice.label, "Zoom in to see airspace.")
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: zoomIn).count, 1)
        sleep(1)
        keepScreenshot("map-airspace-3-too-wide")
        setAirspace(on: false, in: app)
        let other = NSPredicate(format: "label != %@", "Zoom in to see airspace.")
        expectation(for: other, evaluatedWith: notice)
        waitForExpectations(timeout: 15)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: zoomIn).count, 1)
        sleep(1)
        keepScreenshot("map-airspace-4-too-wide-off")
    }

    /// Today, against a Venti server in development mode: the take-offs
    /// around the Tegernsee, each with what the recorded forecast says
    /// about flying from it.
    func testTodaySaysWhatTheForecastSaysOfTheTakeoffs() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        // The cockpit this test opens asks for the position: as on a new
        // iPhone, so that the system's question comes and is answered.
        app.resetAuthorizationStatus(for: .location)
        app.launchArguments = ["--ui-testing", "--bundled-map", "--server", server]
        app.launch()

        // The app opens on Today: the best take-off as a card, with its
        // status in words once its forecast is read.
        let card = element("today-card", in: app)
        XCTAssertTrue(card.waitForExistence(timeout: 60))
        let status = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN %@", ["Flyable", "Marginal", "Not flyable"]))
            .firstMatch
        XCTAssertTrue(status.waitForExistence(timeout: 90))
        // Its readings, and the other take-offs as rows.
        XCTAssertTrue(labelled(containing: "Cloud base", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(element("today-row", in: app).exists)
        // Whose forecast it is, and that it is a reading of a model.
        XCTAssertTrue(labelled(containing: "the take-off decides", in: app).exists)
        sleep(2)
        keepScreenshot("today-1")

        // Another day.
        let days = app.buttons.matching(identifier: "today-days")
        if days.count > 1 {
            days.element(boundBy: days.count - 1).tap()
            sleep(1)
            keepScreenshot("today-2-another-day")
        }

        // A take-off's own page: its status again, and the two things to
        // do from here.
        card.tap()
        XCTAssertTrue(element("site-plan-route", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(element("site-status", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(app.buttons["Plan route"].exists)
        sleep(1)
        keepScreenshot("today-3-site")

        // "Start flight" brings the Fly tab to the front and opens the
        // cockpit, which waits for the take-off.
        app.buttons["Start flight"].tap()
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow While Using App"]
        if allow.waitForExistence(timeout: 10) {
            allow.tap()
        }
        XCTAssertTrue(element("cockpit-end", in: app).waitForExistence(timeout: 20))
        sleep(1)
        keepScreenshot("today-4-cockpit-from-the-site")
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))

        // The site's page is where it was.
        openTab("Today", in: app)
        XCTAssertTrue(app.buttons["Plan route"].waitForExistence(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(card.waitForExistence(timeout: 15))
    }

    /// Foehn on Today, against a Venti server in development mode, whose
    /// pressures put Bolzano and Innsbruck at "likely" and Klagenfurt and
    /// Salzburg at no sign: the core's one line over the take-offs, the
    /// view behind it with both pairs and the chart of the last 24 hours,
    /// and the way to the same view at the page's foot. Light, dark and
    /// with larger text.
    func testTodaySaysWhenFoehnIsLikelyAndShowsThePressures() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        // Light, dark, and with the text size above the largest standard
        // one.
        for (dark, larger) in [(false, false), (true, false), (false, true)] {
            let app = launch(
                bundledMap: true, dark: dark, extra: ["--server", server] + (larger ? Self.largerText : [])
            )
            let suffix = dark ? "-dark" : larger ? "-larger" : ""
            openTab("Today", in: app)
            XCTAssertTrue(element("today-card", in: app).waitForExistence(timeout: 60))

            // The line, over the take-offs, in the core's words.
            let line = element("today-foehn", in: app)
            XCTAssertTrue(line.waitForExistence(timeout: 60))
            XCTAssertTrue(line.label.contains("Bolzano"), line.label)
            XCTAssertTrue(line.label.contains("Innsbruck"), line.label)
            XCTAssertLessThan(line.frame.minY, element("today-card", in: app).frame.minY)
            sleep(2)
            keepScreenshot("foehn-1-today\(suffix)")

            // The view behind it: the first pair with its level, its words
            // and its chart.
            line.tap()
            XCTAssertTrue(app.navigationBars["Foehn"].waitForExistence(timeout: 15))
            XCTAssertTrue(element("foehn-row", in: app).waitForExistence(timeout: 15))
            XCTAssertTrue(labelled(containing: "Bolzano – Innsbruck", in: app).exists)
            XCTAssertTrue(element("foehn-level", in: app).exists)
            XCTAssertTrue(element("foehn-chart", in: app).waitForExistence(timeout: 10))
            sleep(1)
            keepScreenshot("foehn-2-view\(suffix)")

            // The second pair, further down, and under the pairs what the
            // number means, the caution and whose data it is.
            let second = labelled(containing: "Klagenfurt – Salzburg", in: app)
            for _ in 0..<6 where !second.exists {
                app.swipeUp()
            }
            XCTAssertTrue(second.exists)
            sleep(1)
            keepScreenshot("foehn-3-second-pair\(suffix)")
            let credits = element("foehn-credits", in: app)
            for _ in 0..<8 where !credits.exists {
                app.swipeUp()
            }
            XCTAssertTrue(credits.exists)
            XCTAssertTrue(element("foehn-caution", in: app).exists)
            sleep(1)
            keepScreenshot("foehn-4-what-it-means\(suffix)")
            element("foehn-done", in: app).tap()
            XCTAssertTrue(line.waitForExistence(timeout: 10))

            // The same view from the page's foot.
            let row = element("today-foehn-row", in: app)
            for _ in 0..<12 where !(row.exists && isOnScreen(row, in: app)) {
                app.swipeUp()
            }
            XCTAssertTrue(row.exists)
            sleep(1)
            keepScreenshot("foehn-5-foot\(suffix)")
            // Clear of the tab bar, which lies over the page's foot.
            if row.frame.maxY > app.windows.firstMatch.frame.maxY - 120 {
                app.swipeUp()
            }
            row.tap()
            XCTAssertTrue(element("foehn-row", in: app).waitForExistence(timeout: 15))
            element("foehn-done", in: app).tap()
            app.terminate()
        }
    }

    /// The pilot's own take-offs, against a Venti server in development
    /// mode: a take-off of Today is kept on its page, which also says what
    /// its source says of a fee and names its web page; Today then begins
    /// with it under "Your take-offs"; the settings list it; and forgotten
    /// there, Today is as it was.
    func testTodayBeginsWithThePilotsOwnTakeoffs() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        // The map tab opens over the Wallberg, for the test's last step.
        app.launchArguments = [
            "--ui-testing", "--bundled-map", "--server", server, "--map-at", "47.6653,11.7954,14",
        ]
        app.launch()

        // Nothing kept: Today names no groups.
        XCTAssertTrue(element("today-card", in: app).waitForExistence(timeout: 60))
        let ownTitle = element("today-kept-title", in: app)
        XCTAssertFalse(ownTitle.exists)
        XCTAssertFalse(element("today-near-title", in: app).exists)

        // The Jägerkamp, one of the rows: its source names a fee and a
        // web page.
        let row = app.descendants(matching: .any).matching(identifier: "today-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        bringUp(row, in: app)
        row.tap()
        XCTAssertTrue(element("site-plan-route", in: app).waitForExistence(timeout: 15))

        // Further down, the button that keeps it.
        let keep = element("site-keep", in: app)
        bringUp(keep, in: app)
        XCTAssertTrue(keep.waitForExistence(timeout: 10))
        XCTAssertEqual(keep.label, "Keep as my take-off")
        keep.tap()
        let kept = app.buttons["Kept"]
        XCTAssertTrue(kept.waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "Kept on this device only", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("own-1-site-kept")

        // And what its source says besides: the fee in the core's
        // sentence, and the web page as a link that leaves the app.
        let page = element("site-website", in: app)
        bringUp(page, in: app)
        XCTAssertTrue(page.waitForExistence(timeout: 10))
        XCTAssertTrue(page.label.contains("www.dhv.de"), page.label)
        XCTAssertTrue(app.staticTexts["A fee is asked for using this site."].exists)
        sleep(1)
        keepScreenshot("own-2-site-more")

        // Back on Today it comes first, under the core's name for the
        // pilot's own, and the others under theirs.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(ownTitle.waitForExistence(timeout: 30))
        XCTAssertEqual(ownTitle.label, "Your take-offs")
        let nearTitle = element("today-near-title", in: app)
        XCTAssertTrue(nearTitle.waitForExistence(timeout: 10))
        XCTAssertEqual(nearTitle.label, "Within 30 km")
        let own = labelled(containing: "Jägerkamp", in: app)
        XCTAssertTrue(own.waitForExistence(timeout: 10))
        XCTAssertGreaterThan(own.frame.minY, ownTitle.frame.minY)
        XCTAssertLessThan(own.frame.minY, nearTitle.frame.minY)
        sleep(2)
        keepScreenshot("own-3-today")
        drawUp(in: app)
        sleep(1)
        keepScreenshot("own-4-today-the-others")

        // The settings list the pilot's own, with the core's note.
        app.buttons["Settings"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 15))
        let link = element("kept-sites-link", in: app)
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("own-5-settings")
        link.tap()
        let listed = app.descendants(matching: .any).matching(identifier: "kept-site-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(listed.waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Kept on this device only", in: app).exists)
        XCTAssertTrue(labelled(containing: "OpenStreetMap", in: app).exists)
        sleep(1)
        keepScreenshot("own-6-list")

        // Forgotten with a swipe: the list is empty and says what to do.
        listed.swipeLeft()
        XCTAssertTrue(app.buttons["Forget"].waitForExistence(timeout: 10))
        app.buttons["Forget"].tap()
        XCTAssertTrue(app.staticTexts["No take-offs of your own yet"].waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("own-7-list-empty")

        // And Today is as it was: no groups.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("today-card", in: app).waitForExistence(timeout: 30))
        let gone = expectation(for: NSPredicate(format: "exists == false"), evaluatedWith: ownTitle)
        wait(for: [gone], timeout: 30)
        XCTAssertFalse(element("today-near-title", in: app).exists)

        // One of the Wallberg's two ramps, which have no name but a
        // description in their mapper's words: from the map's list. Kept
        // as a screenshot when it is reached; the test does not depend
        // on it.
        openTab("Map", in: app)
        guard waitForCredit("Sites", in: app), openLayers(in: app) else { return }
        let sites = app.buttons["Flying sites"]
        guard sites.waitForExistence(timeout: 30) else { return }
        sites.tap()
        let ramp = app.descendants(matching: .any).matching(identifier: "site-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Take-off", "47.665"))
            .firstMatch
        guard ramp.waitForExistence(timeout: 15) else { return }
        bringUp(ramp, in: app)
        ramp.tap()
        guard app.buttons["Plan route"].waitForExistence(timeout: 15) else { return }
        let description = element("site-description", in: app)
        bringUp(description, in: app)
        if description.exists {
            sleep(1)
            keepScreenshot("own-8-site-description")
        }
    }

    /// The air around and above a take-off, against a Venti server in
    /// development mode, whose recorded forecasts of late November have a
    /// neighbourhood with strong gusts on some hours and stable air on
    /// most: Today says a day's line about the wind near by with the
    /// take-off it is about, and what that is; the take-off's page draws
    /// the hours' layers into the picture of the day's air and says an
    /// hour's layers and its wind near by in words when the hour is
    /// tapped. What is asserted is what the recording has, by its shape
    /// and not by its figures.
    ///
    /// A take-off's page speaks of its first day with hours left, and by
    /// the development server's clock that is the recorded Saturday, a
    /// day of rain whose air is mixed all the way up. So the test names
    /// the moment itself (`--today-at`): the Sunday evening, after which
    /// the page speaks of the Monday, whose wind near by is much stronger
    /// and whose air is stable; and for the picture in the dark the
    /// Saturday evening, for the Sunday, which has an inversion as well.
    func testATakeoffsPageShowsTheAirAroundAndAbove() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "--ui-testing", "--bundled-map", "--server", server, "--today-at", "2026-11-22T18:00:00Z",
        ]
        app.launch()

        // Today, with every forecast read.
        let card = element("today-card", in: app)
        XCTAssertTrue(card.waitForExistence(timeout: 60))
        let status = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN %@", ["Flyable", "Marginal", "Not flyable"]))
            .firstMatch
        XCTAssertTrue(status.waitForExistence(timeout: 90))
        sleep(4)

        // The day's line: on the first of the days that has one for some
        // take-off. A card says it short, the core's `nearbyShort`: "Gusts to 55
        // km/h near by at 14:00"; the take-off's page says it in full.
        let line = labelled(containing: " near by at ", in: app)
        let days = app.buttons.matching(identifier: "today-days")
        var dayWithLine: Int?
        var said: [String] = []
        for index in 0..<max(days.count, 1) {
            if days.count > 1 {
                days.element(boundBy: index).tap()
                sleep(1)
            }
            let takeoffs = app.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier IN %@", ["today-card", "today-row"]))
            let lines = takeoffs.matching(NSPredicate(format: "label CONTAINS %@", " near by at "))
            said.append("day \(index): \(lines.count) of \(takeoffs.count) take-offs")
            if dayWithLine == nil, line.exists {
                dayWithLine = index
            }
        }
        // How often the line speaks in the recording, for whoever reads
        // the log.
        print("The wind near by, day by day: \(said.joined(separator: "; "))")
        let found = try XCTUnwrap(dayWithLine, "No day of the recording has a line about the wind near by")
        if days.count > 1 {
            days.element(boundBy: found).tap()
            sleep(1)
        }
        XCTAssertTrue(line.waitForExistence(timeout: 10))
        XCTAssertNotNil(
            line.label.range(
                of: #"(Gusts to|Wind) \d+ km/h near by at \d{1,2}:00"#,
                options: .regularExpression
            ),
            line.label
        )
        // And at the foot, what the wind near by is and that it judges
        // nothing.
        XCTAssertTrue(labelled(containing: "Near by is the strongest wind and gust", in: app).exists)
        XCTAssertTrue(labelled(containing: "it changes no verdict", in: app).exists)
        bringUp(line, in: app)
        sleep(1)
        keepScreenshot("air-1-today-the-days-line")

        // The take-off the line is about: its own page.
        line.tap()
        XCTAssertTrue(element("site-plan-route", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(element("site-status", in: app).waitForExistence(timeout: 30))
        // The page speaks of the day chosen on Today: the line stands
        // under the day's own.
        let onPage = element("site-nearby", in: app)
        XCTAssertTrue(onPage.waitForExistence(timeout: 10))
        XCTAssertTrue(onPage.label.contains("Much stronger near by at"), onPage.label)
        sleep(1)
        keepScreenshot("air-2-site-the-days-line")

        // Further down, the picture of the day's air, with the hours to
        // tap over it.
        let chart = element("air-chart", in: app)
        for _ in 0..<4 where !chart.exists {
            app.swipeUp()
        }
        XCTAssertTrue(chart.waitForExistence(timeout: 15))
        let hours = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", "air-hour-"))
        XCTAssertGreaterThan(hours.count, 1)
        // What each hour says, as a screen reader hears it.
        let spoken = hours.allElementsBoundByIndex.map { ($0.value as? String) ?? "" }
        print("The hours of the picture: \(spoken)")
        // Late November: the air is stable or inverted in most hours, and
        // the forecast map's daylight hours have a wind near by.
        let layered = spoken.filter { $0.contains("Stable layer between") || $0.contains("Inversion between") }
        XCTAssertGreaterThan(layered.count, spoken.count / 2, "\(spoken)")
        XCTAssertTrue(spoken.contains { $0.contains("Gusts near by: ") }, "\(spoken)")

        // The picture whole, clear of the tab bar, with the words of the
        // hour it opens on under it.
        let detail = element("air-hour-clock", in: app)
        bringUp(detail, in: app)
        XCTAssertTrue(detail.waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("air-3-the-picture-with-layers")

        // An hour tapped: its layers in the core's words, each beside its
        // band, and the strongest wind and gust near by with where. An
        // hour in which the wind near by is much stronger where the day
        // has one, else the first with layers and figures.
        let hasLine = { (said: String) in
            said.range(of: #"^(Gusts to|Wind) \d+ km/h .+, \d+ km/h at the place"#, options: .regularExpression) != nil
        }
        let wanted = spoken.firstIndex(where: hasLine)
            ?? spoken.firstIndex { $0.contains("layer between") && $0.contains("Gusts near by: ") }
            ?? spoken.firstIndex { $0.contains("Gusts near by: ") }
        let index = try XCTUnwrap(wanted)
        hours.element(boundBy: index).tap()
        usleep(600_000)
        let figure = app.descendants(matching: .any).matching(identifier: "air-hour-figure")
        XCTAssertTrue(figure.firstMatch.waitForExistence(timeout: 10))
        let gusts = figure.matching(NSPredicate(format: "label == %@", "Gusts near by")).firstMatch
        XCTAssertTrue(gusts.exists)
        let figureWords = (gusts.value as? String) ?? ""
        XCTAssertNotNil(
            figureWords.range(of: #"^\d+ km/h (\d+ km to the [a-z-]+|in the square around the place)"#, options: .regularExpression),
            figureWords
        )
        if hasLine(spoken[index]) {
            let hourLine = element("air-hour-line", in: app)
            XCTAssertTrue(hourLine.exists)
            XCTAssertTrue(hourLine.label.hasPrefix("Gusts to") || hourLine.label.hasPrefix("Wind "), hourLine.label)
        }
        if spoken[index].contains("layer between") || spoken[index].contains("Inversion between") {
            let layer = element("air-hour-layer", in: app)
            XCTAssertTrue(layer.exists)
            XCTAssertNotNil(
                layer.label.range(
                    of: #"(Stable layer|Inversion) between \d+ and \d+ m: \d+\.\d °C"#, options: .regularExpression
                ),
                layer.label
            )
        }
        bringUp(detail, in: app)
        sleep(1)
        keepScreenshot("air-4-an-hour-in-words")

        // At the page's end, behind "How this is read": what the layers
        // and the wind near by are and are not.
        unfoldHowASiteIsRead(in: app)
        let layersNote = labelled(containing: "Layers are read between the model's levels", in: app)
        bringUp(layersNote, in: app)
        XCTAssertTrue(layersNote.waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "Near by is the strongest wind and gust", in: app).exists)
        sleep(1)
        keepScreenshot("air-5-the-notes")

        // The picture once more in the dark, on the day before, whose air
        // has an inversion at the ground under its stable layer: the bands
        // are in the dark theme's muted ink.
        app.terminate()
        app.launchArguments = [
            "--ui-testing", "--bundled-map", "--dark", "--server", server, "--today-at", "2026-11-21T18:00:00Z",
        ]
        app.launch()
        XCTAssertTrue(card.waitForExistence(timeout: 60))
        XCTAssertTrue(status.waitForExistence(timeout: 90))
        sleep(4)
        card.tap()
        XCTAssertTrue(element("site-status", in: app).waitForExistence(timeout: 30))
        for _ in 0..<4 where !chart.exists {
            app.swipeUp()
        }
        XCTAssertTrue(chart.waitForExistence(timeout: 15))
        let inTheDark = hours.allElementsBoundByIndex.map { ($0.value as? String) ?? "" }
        print("The hours of the picture in the dark: \(inTheDark)")
        XCTAssertTrue(inTheDark.contains { $0.contains("Inversion between") }, "\(inTheDark)")
        if let inverted = inTheDark.firstIndex(where: { $0.contains("Inversion between") }) {
            hours.element(boundBy: inverted).tap()
            usleep(600_000)
        }
        bringUp(detail, in: app)
        sleep(1)
        keepScreenshot("air-6-the-picture-in-the-dark")
    }

    /// Unfolds the row at the end of a site's page that keeps how the page
    /// is read, what a site on a map is worth, and the source's notice
    /// and credit.
    private func unfoldHowASiteIsRead(in app: XCUIApplication) {
        let row = element("site-how-read", in: app)
        for _ in 0..<10 where !(row.exists && isOnScreen(row, in: app)) {
            app.swipeUp()
        }
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.tap()
        usleep(800_000)
    }

    /// Draws the page up until an element stands clear of the tab bar.
    private func bringUp(_ element: XCUIElement, in app: XCUIApplication) {
        let bottom = app.windows.firstMatch.frame.maxY - 110
        for _ in 0..<8 where !(element.exists && !element.frame.isEmpty && element.frame.maxY < bottom) {
            drawUp(in: app)
        }
    }

    /// Draws the page up by a third of the screen, gently: a swipe flings
    /// a list past what a test looks for.
    private func drawUp(in app: XCUIApplication) {
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
            .press(forDuration: 0.2, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4)))
        sleep(1)
    }

    /// Today on a strong summer day, which no recording has: against a
    /// stand-in server that hands on a development server's answers and
    /// makes up the forecast (`tools/made-up-day`). The day is made up;
    /// what is tested is that the screens say what the core reads from it.
    /// Today's days as the forecast reaches them, four at most and three
    /// where four do not fit whole, side by side at a larger text size (the step above the largest of the
    /// standard sizes, about 135 %), against the stand-in with a made-up
    /// day, whose forecast has four days.
    func testTodaysDaysFitWithLargerText() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "--ui-testing", "--bundled-map", "--server", server,
            "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL",
        ]
        app.launch()
        let days = app.buttons.matching(identifier: "today-days")
        XCTAssertTrue(days.firstMatch.waitForExistence(timeout: 90))
        // Every forecast read: the days are those of the longest.
        XCTAssertTrue(element("today-card", in: app).waitForExistence(timeout: 60))
        sleep(4)
        // Four where their names fit whole, else three.
        XCTAssertTrue([3, 4].contains(days.count), "\(days.count) days")
        keepScreenshot("today-8-days-larger-text")
    }

    func testTodaySaysWhatKindOfDayTheThermalsMake() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["--ui-testing", "--bundled-map", "--server", server]
        app.launch()

        // The best take-off is the one with the best thermals: its card
        // says the kind of day with its figures, and the head says it
        // over the valley.
        let card = element("today-card", in: app)
        XCTAssertTrue(card.waitForExistence(timeout: 60))
        let kind = element("today-kind-of-day", in: app)
        XCTAssertTrue(kind.waitForExistence(timeout: 90))
        XCTAssertEqual(kind.label, "Big day")
        XCTAssertTrue(labelled(containing: "m/s to a cloud base of 3500 m", in: app).waitForExistence(timeout: 15))
        sleep(2)
        keepScreenshot("today-5-big-day")

        // Further down: the hours with their climbs, and the other
        // take-offs with their kinds of day.
        app.swipeUp()
        sleep(1)
        keepScreenshot("today-6-kinds-of-day")

        // The take-off's own page says it too.
        app.swipeDown()
        XCTAssertTrue(card.waitForExistence(timeout: 15))
        card.tap()
        XCTAssertTrue(element("site-status", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(element("thermals", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("today-7-site-on-a-big-day")

        // Further down it draws the day's air: the thermals hour by hour
        // and the wind at heights.
        let chart = element("air-chart", in: app)
        for _ in 0..<4 where !chart.exists {
            app.swipeUp()
        }
        XCTAssertTrue(chart.waitForExistence(timeout: 15))
        // Its head is on screen already: the page is drawn up gently, not
        // flung, until the whole picture stands clear of the tab bar.
        let bottom = app.windows.firstMatch.frame.maxY
        for _ in 0..<4 where chart.frame.maxY > bottom - 120 {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.2, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)))
            sleep(1)
        }
        sleep(1)
        keepScreenshot("today-8-the-air")

        // Under the picture, the forecast for the place, with the two rows
        // of thermals at the foot of its table. Kept as a screenshot when
        // it is reached; the test does not depend on it.
        let forecast = element("site-forecast", in: app)
        for _ in 0..<4 where !(forecast.exists && forecast.isHittable) {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.2, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35)))
            sleep(1)
        }
        if forecast.exists, forecast.isHittable {
            forecast.tap()
            let table = element("forecast-table", in: app)
            if table.waitForExistence(timeout: 60) {
                // The rows of thermals are the table's last: the table's
                // hours scroll sideways, its rows stand in the page.
                _ = element("forecast-thermals-note", in: app).waitForExistence(timeout: 10)
                sleep(1)
                keepScreenshot("forecast-thermals")
            }
        }
    }

    /// The forecast map on the map tab: the kind of day square by square,
    /// against the stand-in server with a made-up map (`tools/made-up-day`).
    /// The map is made up; what is tested is that the layer, its days and
    /// its key come up, and what a square says when one is tapped.
    func testTheMapShowsTheKindOfDaySquareBySquare() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))

        // The choices are kept between launches: an earlier run that
        // stopped half-way may have left the other layers off.
        for layer in ["layer-wind", "layer-sites", "layer-pilots"] {
            setLayer(layer, on: true, in: app)
        }
        setLayer("layer-airspace", on: false, in: app)
        setLayer("forecast-chip", on: false, in: app)

        // And it may have left this layer on.
        let chip = element("forecast-chip", in: app)
        XCTAssertTrue(chip.waitForExistence(timeout: 30))
        if chip.isSelected {
            chip.tap()
            usleep(700_000)
        }
        XCTAssertFalse(chip.isSelected)
        chip.tap()
        XCTAssertTrue(chip.waitForExistence(timeout: 10))

        // The card at the map's lower edge: the days and the key, once the
        // map is there. On "Thermals", which an earlier run of the hours'
        // test may have left on another layer.
        if element("forecast-layer-thermals", in: app).waitForExistence(timeout: 60) {
            chooseForecastLayer("thermals", in: app)
        }
        XCTAssertTrue(element("forecast-days", in: app).waitForExistence(timeout: 90))
        XCTAssertTrue(element("forecast-hours", in: app).waitForExistence(timeout: 60))
        // The card as it opens: the key and the notes are not on it.
        XCTAssertFalse(element("forecast-key", in: app).exists)
        // Time for the squares to be drawn.
        sleep(4)
        keepScreenshot("map-forecast-1-today")
        // Behind "Key and notes": the key, the model and its run.
        showKeyAndNotes(true, in: app)
        XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Not flyable", in: app).exists)
        XCTAssertTrue(element("forecast-model", in: app).exists)
        sleep(1)
        keepScreenshot("map-forecast-1b-key-and-notes")
        // Closed again: open, the card lies where a square is tapped below.
        showKeyAndNotes(false, in: app)

        // The second day. The stand-in dates its map from the development
        // server's clock, which the app names the days by, so they read
        // "Today" and "Tomorrow"; which day is today also hangs on the
        // simulator's time zone, so the test goes by the second segment.
        let tomorrow = element("forecast-days", in: app).buttons.element(boundBy: 1)
        XCTAssertTrue(tomorrow.waitForExistence(timeout: 15))
        tomorrow.tap()
        usleep(700_000)
        XCTAssertTrue(tomorrow.isSelected)
        sleep(3)
        keepScreenshot("map-forecast-2-tomorrow")

        // A square, tapped. The wind, the sites and the pilots are switched
        // off first, since a tap on one of them opens its page instead (the
        // development server's glider flies over the made-up map's middle);
        // they are switched on again before anything here may fail. The
        // tap is a little above the map's middle, where the made-up map
        // has squares (47.45 to 47.95 N, 11.45 to 12.35 E). Only a square's
        // sheet is checked, and only when it opens.
        for layer in ["layer-wind", "layer-sites", "layer-pilots"] {
            setLayer(layer, on: false, in: app)
        }
        sleep(2)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)).tap()
        let square = element("forecast-square", in: app)
        var status: String?
        /// Which of the forecast's days was chosen when it opened, counted
        /// from the first.
        var forecastOpenedOn: Int?
        if square.waitForExistence(timeout: 15) {
            let said = element("forecast-square-status", in: app)
            if said.waitForExistence(timeout: 10) {
                status = said.label
            }
            sleep(1)
            keepScreenshot("map-forecast-3-square")
            // The forecast for the square's place, kept when it comes. It
            // opens on the day the map showed, the second: the second of
            // the forecast's days too, since both begin today.
            let toForecast = element("forecast-square-forecast", in: app)
            if toForecast.exists, toForecast.isHittable {
                toForecast.tap()
                if element("forecast-table", in: app).waitForExistence(timeout: 60) {
                    let days = element("forecast-day", in: app).buttons
                    if days.firstMatch.waitForExistence(timeout: 10) {
                        forecastOpenedOn = (0..<days.count).first { days.element(boundBy: $0).isSelected }
                    }
                    sleep(1)
                    keepScreenshot("map-forecast-4-forecast")
                }
                // Back to the square's sheet, which has the button to close it.
                for _ in 0..<2 where !app.buttons["Done"].waitForExistence(timeout: 5) {
                    if app.navigationBars.buttons.firstMatch.exists {
                        app.navigationBars.buttons.firstMatch.tap()
                    }
                }
            }
        }
        // Whatever opened, closed again: by its button, or by pulling the
        // sheet down. While a sheet is up the map's round buttons are not
        // to be had.
        if !app.buttons["Show my position"].isHittable {
            if app.buttons["Done"].exists {
                app.buttons["Done"].tap()
            } else {
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.53))
                    .press(
                        forDuration: 0.3, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.98))
                    )
            }
            sleep(1)
        }
        for layer in ["layer-wind", "layer-sites", "layer-pilots"] {
            setLayer(layer, on: true, in: app)
        }
        // The square's status, as a word the core gives.
        if let status {
            XCTAssertTrue(["Flyable", "Marginal", "Not flyable"].contains(status), status)
        }

        // Off again, as the next test expects the map.
        XCTAssertTrue(chip.waitForExistence(timeout: 15))
        setLayer("forecast-chip", on: false, in: app)
        XCTAssertFalse(chip.isSelected)
        XCTAssertFalse(element("forecast-card", in: app).exists)
        // The square's forecast, on the map's day: tomorrow. Asked last,
        // so that a failure leaves the layers as the next test expects.
        if let forecastOpenedOn {
            XCTAssertEqual(forecastOpenedOn, 1, "The square's forecast did not open on the map's second day.")
        }
    }

    /// Where the map opens for the screenshots of the hours: the made-up
    /// map's squares (47.45 to 47.95 N, 11.45 to 12.35 E, nine by five),
    /// all of them in the part of the map the card leaves to be seen.
    private static let overTheMadeUpSquares = "47.44,11.9,8.1"

    /// Chooses a layer of the forecast map's card by its chip. The row
    /// scrolls inside the card: a chip that is not wholly in it is
    /// brought into it first.
    private func chooseForecastLayer(_ layer: String, in app: XCUIApplication) {
        let chip = element("forecast-layer-\(layer)", in: app)
        XCTAssertTrue(chip.waitForExistence(timeout: 15))
        let row = element("forecast-layers", in: app)
        for _ in 0..<3 where !row.frame.insetBy(dx: -1, dy: -1).contains(chip.frame) {
            if chip.frame.midX > row.frame.midX {
                row.swipeLeft()
            } else {
                row.swipeRight()
            }
            usleep(600_000)
        }
        chip.tap()
    }

    /// Chooses an hour on the card's line of hours by what the strip
    /// says of it ("13:00"), and says whether it came to that. The strip
    /// is one element, and how many hours a day has, and whether "Day"
    /// stands before them, differs: so it is tapped from where the hour
    /// should stand outwards, in steps narrower than a cell.
    @discardableResult
    private func chooseHour(_ hour: String, in app: XCUIApplication) -> Bool {
        let strip = element("forecast-hours", in: app)
        guard strip.waitForExistence(timeout: 60) else { return false }
        for place in [0.65, 0.59, 0.71, 0.53, 0.77, 0.47, 0.83, 0.41, 0.89, 0.35, 0.95, 0.29, 0.23, 0.17] {
            if strip.value as? String == hour {
                return true
            }
            strip.coordinate(withNormalizedOffset: CGVector(dx: place, dy: 0.5)).tap()
            usleep(400_000)
        }
        return strip.value as? String == hour
    }

    /// Opens or closes "Key and notes" on the forecast map's card.
    private func showKeyAndNotes(_ shown: Bool, in app: XCUIApplication) {
        let more = element("forecast-more", in: app)
        XCTAssertTrue(more.waitForExistence(timeout: 15))
        if (more.value as? String == "Shown") != shown {
            more.tap()
            usleep(600_000)
        }
        XCTAssertEqual(more.value as? String, shown ? "Shown" : "Hidden")
    }

    /// Chooses the height of the forecast map's wind from its menu.
    private func chooseForecastLevel(_ name: String, in app: XCUIApplication) {
        let level = element("forecast-level", in: app)
        XCTAssertTrue(level.waitForExistence(timeout: 15))
        level.tap()
        let height = app.buttons[name]
        XCTAssertTrue(height.waitForExistence(timeout: 10))
        height.tap()
        XCTAssertTrue(element("forecast-hours", in: app).waitForExistence(timeout: 60))
    }

    /// The forecast map by the hour, against the stand-in with a made-up
    /// day (`tools/made-up-day`): the card's row of layers, the line of
    /// hours, each of the five layers at 13:00, the wind at a height, the
    /// key and the notes behind their line, and a tapped square of an
    /// hour with its days side by side. The day is made up. The map opens
    /// over the made-up squares, all of them above the card. The layer,
    /// the level and the chip are kept between launches: the test starts
    /// on "Thermals" at the ground and leaves it so, off.
    func testTheForecastMapShowsTheHoursOfADay() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", Self.overTheMadeUpSquares])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        // Nothing over the squares: the wind, the airspace, the sites and
        // the pilots off.
        let others = ["layer-wind", "layer-sites", "layer-pilots"]
        for layer in others {
            setLayer(layer, on: false, in: app)
        }
        setLayer("layer-airspace", on: false, in: app)
        setLayer("forecast-chip", on: true, in: app)
        XCTAssertTrue(element("forecast-chip", in: app).isSelected)
        chooseForecastLayer("thermals", in: app)

        // The day as a whole, and the line of hours under the days; the
        // key and the notes are not on the card as it opens.
        let strip = element("forecast-hours", in: app)
        let words = element("forecast-words", in: app)
        XCTAssertTrue(strip.waitForExistence(timeout: 60))
        XCTAssertEqual(strip.value as? String, "The whole day")
        XCTAssertTrue(words.waitForExistence(timeout: 15))
        XCTAssertEqual(words.label, "Thermals, the whole day")
        XCTAssertFalse(element("forecast-key", in: app).exists)
        XCTAssertFalse(element("forecast-level", in: app).exists)
        sleep(3)
        keepScreenshot("map-hours-1-thermals-day")

        // An hour, by a tap on the strip: one o'clock.
        XCTAssertTrue(chooseHour("13:00", in: app), "The strip says \(String(describing: strip.value))")
        XCTAssertEqual(words.label, "Thermals, 13:00")
        sleep(3)
        keepScreenshot("map-hours-2-thermals-hour")

        // The other layers, each opening at the core's first hour and
        // then set to one o'clock, when the made-up day has rain, gusts
        // and cloud in it.
        for (layer, name) in [("wind", "3-wind"), ("gusts", "4-gusts"), ("cloud", "5-cloud"), ("rain", "6-rain")] {
            chooseForecastLayer(layer, in: app)
            XCTAssertTrue(strip.waitForExistence(timeout: 60))
            XCTAssertTrue(chooseHour("13:00", in: app), "\(layer): the strip says \(String(describing: strip.value))")
            XCTAssertTrue(words.label.hasSuffix(", 13:00"), words.label)
            // The height's menu stands beside the words with the wind alone.
            XCTAssertEqual(element("forecast-level", in: app).exists, layer == "wind")
            sleep(3)
            keepScreenshot("map-hours-\(name)")
        }

        // The wind at a height, from the level's menu beside the words.
        chooseForecastLayer("wind", in: app)
        chooseForecastLevel("2000 m", in: app)
        XCTAssertTrue(chooseHour("13:00", in: app), "The strip says \(String(describing: strip.value))")
        XCTAssertEqual(words.label, "Wind at 2000 m, 13:00")
        sleep(3)
        keepScreenshot("map-hours-7-wind-2000")

        // The key, the model's run and the notes, behind their line.
        showKeyAndNotes(true, in: app)
        XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(element("forecast-model", in: app).exists)
        sleep(1)
        keepScreenshot("map-hours-7b-key-and-notes")
        showKeyAndNotes(false, in: app)
        XCTAssertFalse(element("forecast-key", in: app).exists)

        // At 1000 m: where the made-up ground lies higher there is no
        // wind, and a tapped square says so. Squares are tapped across
        // the map above the card until one says it; each opens a sheet.
        chooseForecastLevel("1000 m", in: app)
        sleep(2)
        let sheet = element("forecast-hour-square", in: app)
        var none = false
        var kept = false
        search: for row in [0.3, 0.38, 0.22, 0.34, 0.26] {
            for column in [0.5, 0.3, 0.7, 0.15, 0.85] {
                app.coordinate(withNormalizedOffset: CGVector(dx: column, dy: row)).tap()
                guard sheet.waitForExistence(timeout: 5) else { continue }
                let title = element("forecast-hour-title", in: app).label
                XCTAssertTrue(element("forecast-square-days", in: app).waitForExistence(timeout: 10))
                if title.contains("No wind at 1000 m") {
                    if !none {
                        sleep(1)
                        keepScreenshot("map-hours-9-no-wind-at-1000")
                    }
                    none = true
                } else {
                    if !kept {
                        // A square with a wind: its arrow, and its days
                        // side by side.
                        sleep(1)
                        keepScreenshot("map-hours-8-square")
                    }
                    kept = true
                }
                app.buttons["Done"].tap()
                _ = sheet.waitForNonExistence(timeout: 5)
                if none, kept {
                    break search
                }
            }
        }

        // The wind at the ground again, at one o'clock and a zoom closer
        // (a double tap on the map), where the arrows have their labels:
        // a screenshot alone, kept if the strip comes to one o'clock.
        chooseForecastLevel("Ground", in: app)
        if chooseHour("13:00", in: app) {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).doubleTap()
            sleep(3)
            keepScreenshot("map-hours-9b-wind-with-labels")
        }

        // Back to "Thermals", and off, as the next test expects the map;
        // then what the squares had to say.
        chooseForecastLayer("thermals", in: app)
        for layer in others {
            setLayer(layer, on: true, in: app)
        }
        setLayer("forecast-chip", on: false, in: app)
        XCTAssertFalse(element("forecast-chip", in: app).isSelected)
        XCTAssertTrue(kept, "No tap opened a square with a wind at 1000 m.")
        XCTAssertTrue(none, "No square said that there is no wind at 1000 m.")
    }

    /// The keys of the forecast map's layers in the dark appearance, the
    /// card over the map, which stays light: the wind's, the cloud's and
    /// the thermals', with "Key and notes" open.
    func testTheForecastMapsKeysInTheDark() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(
            bundledMap: true, dark: true, extra: ["--server", server, "--map-at", Self.overTheMadeUpSquares]
        )
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("forecast-chip", on: true, in: app)
        for (layer, name) in [("wind", "wind"), ("cloud", "cloud"), ("thermals", "thermals")] {
            chooseForecastLayer(layer, in: app)
            XCTAssertTrue(element("forecast-hours", in: app).waitForExistence(timeout: 60))
            XCTAssertTrue(chooseHour("13:00", in: app))
            if layer == "wind" {
                // The card as it opens, in the dark.
                sleep(3)
                keepScreenshot("map-hours-dark-wind-closed")
            }
            showKeyAndNotes(true, in: app)
            XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 30))
            sleep(3)
            keepScreenshot("map-hours-dark-\(name)")
        }
        setLayer("forecast-chip", on: false, in: app)
        XCTAssertFalse(element("forecast-chip", in: app).isSelected)
    }

    /// The forecast map's card at a larger text size (the step above the
    /// largest of the standard sizes, about 135 %), against the stand-in
    /// with a made-up day: the row of layers, the four days with their
    /// names whole, a line of eleven hours, the words with the height's
    /// menu beside them, and the key and the notes open, under the map's
    /// chips. Run on the narrowest simulator with `--first "iPhone 17e"`,
    /// this test being the first by name. What the screenshots show is
    /// for a person to look at; the test holds that the card stays on
    /// the screen with everything on it to be had.
    func testTheForecastCardFitsWithLargerText() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = [
            "--ui-testing", "--bundled-map", "--server", server, "--map-at", Self.overTheMadeUpSquares,
            "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL",
        ]
        app.launch()
        openTab("Map", in: app)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("forecast-chip", on: true, in: app)
        let card = element("forecast-card", in: app)
        let window = app.windows.firstMatch.frame

        // "Thermals": the strip begins with "Day".
        chooseForecastLayer("thermals", in: app)
        XCTAssertTrue(element("forecast-hours", in: app).waitForExistence(timeout: 60))
        XCTAssertEqual(element("forecast-days", in: app).buttons.count, 4)
        sleep(3)
        keepScreenshot("map-hours-larger-text-1-thermals")

        // The wind at a height: the words, and the menu beside them.
        chooseForecastLayer("wind", in: app)
        chooseForecastLevel("2000 m", in: app)
        XCTAssertTrue(chooseHour("13:00", in: app))
        XCTAssertEqual(element("forecast-words", in: app).label, "Wind at 2000 m, 13:00")
        XCTAssertTrue(window.contains(element("forecast-level", in: app).frame))
        sleep(3)
        keepScreenshot("map-hours-larger-text-2-wind-2000")

        // The key and the notes open: the card stays under the map's
        // chips, and its line of hours and its days on the screen.
        showKeyAndNotes(true, in: app)
        XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("map-hours-larger-text-3-key-and-notes")
        let chips = element("forecast-chip", in: app).frame
        let open = card.frame
        showKeyAndNotes(false, in: app)

        // As the next test expects it: the ground, "Thermals", off.
        chooseForecastLevel("Ground", in: app)
        chooseForecastLayer("thermals", in: app)
        setLayer("forecast-chip", on: false, in: app)
        XCTAssertFalse(element("forecast-chip", in: app).isSelected)
        XCTAssertGreaterThanOrEqual(open.minY, chips.maxY, "The open card lies over the map's chips.")
        XCTAssertTrue(window.contains(open), "The open card does not lie on the screen.")
    }

    /// Where the map opens for the radar's screenshots: far enough out for
    /// the whole of the development server's pictures (9.4 to 17.2 E,
    /// 46.3 to 48.5 N, the region's box, for both layers), their edge,
    /// and the analysis filling what the radars do not see, in the part
    /// of the map the card leaves to be seen. The map that comes with the
    /// app ends long before: beyond it the pictures lie on the map's bare
    /// ground.
    private static let overTheRadar = "46.3,13.3,5"

    /// Opens or closes "Key and notes" on the rain radar's card.
    private func showRadarKeyAndNotes(_ shown: Bool, in app: XCUIApplication) {
        let more = element("radar-more", in: app)
        XCTAssertTrue(more.waitForExistence(timeout: 15))
        if (more.value as? String == "Shown") != shown {
            more.tap()
            usleep(600_000)
        }
        XCTAssertEqual(more.value as? String, shown ? "Shown" : "Hidden")
    }

    /// Waits until the radar's line on the card has these words in it,
    /// and says whether it came to that.
    private func radarSays(_ part: String, in app: XCUIApplication, timeout: TimeInterval = 30) -> Bool {
        let words = element("radar-words", in: app)
        let until = Date().addingTimeInterval(timeout)
        while Date() < until {
            if words.exists, words.label.contains(part) {
                return true
            }
            usleep(300_000)
        }
        return false
    }

    /// Whether a line is a moment's of both layers: the radars' time and
    /// age, and the time of the analysis under it. The times are on the
    /// simulator's clock, so they are looked at by their form.
    private func isAMomentOfBothLayers(_ line: String, minutes: Int) -> Bool {
        let time = "\\d\\d:\\d\\d"
        let form = "^Rain radar, \(time), \(minutes) minutes ago\\. Austria's analysis is of \(time)\\.$"
        return line.range(of: form, options: .regularExpression) != nil
    }

    /// The rain radar on the map tab, against the development server,
    /// which has both sources recorded on 2026-11-21: DWD's radars with
    /// frames of 13:10 and 13:15 UTC, GeoSphere Austria's analysis with
    /// frames of 12:45 and 13:00, and a radar's clock that stands at
    /// 13:19. The layer switched on, the card with the two moments on the
    /// newest, both pictures on the map with the radars' on top and the
    /// analysis where the radars see nothing, the key and the notes
    /// behind their line with what each layer is, a step to the moment
    /// before, and the moments played to the newest again. The chip's
    /// choice is kept between launches: the test starts by looking which
    /// way it stands and switches it off at its end.
    func testTheMapShowsTheRainRadar() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", Self.overTheRadar])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        // An earlier run that stopped half-way may have left either on.
        setLayer("forecast-chip", on: false, in: app)
        let chip = element("radar-chip", in: app)
        XCTAssertTrue(chip.waitForExistence(timeout: 30))
        setLayer("radar-chip", on: false, in: app)
        XCTAssertFalse(chip.isSelected)
        XCTAssertFalse(element("radar-card", in: app).exists)

        setLayer("radar-chip", on: true, in: app)
        XCTAssertTrue(chip.isSelected)
        // The card: the two moments, on the newest, the radars' picture
        // four minutes old by the radar's clock, and the line saying of
        // which moment the analysis under it is. The clock's times are
        // the simulator's time zone's.
        let strip = element("radar-frames", in: app)
        XCTAssertTrue(strip.waitForExistence(timeout: 60))
        XCTAssertTrue(radarSays(", 4 minutes ago", in: app, timeout: 60))
        let newest = element("radar-words", in: app).label
        XCTAssertTrue(isAMomentOfBothLayers(newest, minutes: 4), newest)
        XCTAssertEqual((strip.value as? String)?.hasSuffix(", 4 minutes ago"), true)
        XCTAssertTrue(element("radar-play", in: app).exists)
        // The card as it opens: the key and the notes are not on it.
        XCTAssertFalse(element("radar-key", in: app).exists)
        // Both sources' credits are behind the map's credits button while
        // their pictures are drawn.
        XCTAssertTrue(waitForCredit("Radar: Deutscher Wetterdienst and GeoSphere Austria", in: app, timeout: 30))
        // Time for the pictures to be drawn.
        sleep(4)
        keepScreenshot("map-radar-1-newest")

        // Behind "Key and notes": what each layer is, the key from the
        // most rain down, what the radar did not see, the note and both
        // credits.
        showRadarKeyAndNotes(true, in: app)
        let layers = element("radar-layers", in: app)
        XCTAssertTrue(layers.waitForExistence(timeout: 15))
        XCTAssertTrue(layers.label.contains("Germany's radars, every 5 minutes"), layers.label)
        XCTAssertTrue(
            layers.label.contains("Austria's analysis of radar and gauges, every 15 minutes"), layers.label
        )
        XCTAssertTrue(element("radar-key", in: app).exists)
        XCTAssertTrue(labelled(containing: "Cloudburst", in: app).exists)
        XCTAssertTrue(labelled(containing: "Not seen by the radar", in: app).exists)
        let note = element("radar-note", in: app)
        XCTAssertTrue(note.label.contains("Datenbasis: Deutscher Wetterdienst"), note.label)
        XCTAssertTrue(note.label.contains("Data source: GeoSphere Austria"), note.label)
        sleep(1)
        keepScreenshot("map-radar-2-key-and-notes")
        showRadarKeyAndNotes(false, in: app)

        // The moment before: a tap on its cell, the strip's left half.
        // The same analysis lies under the radars' picture before.
        strip.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.5)).tap()
        XCTAssertTrue(radarSays(", 9 minutes ago", in: app))
        let before = element("radar-words", in: app).label
        XCTAssertTrue(isAMomentOfBothLayers(before, minutes: 9), before)
        XCTAssertEqual((strip.value as? String)?.hasSuffix(", 9 minutes ago"), true)
        sleep(2)
        keepScreenshot("map-radar-3-the-moment-before")

        // Played: through the moments, to stop at the newest.
        element("radar-play", in: app).tap()
        let played = radarSays(", 4 minutes ago", in: app)
        sleep(1)
        keepScreenshot("map-radar-4-played-to-the-newest")

        // Off again, as the next test expects the map.
        setLayer("radar-chip", on: false, in: app)
        XCTAssertFalse(chip.isSelected)
        XCTAssertFalse(element("radar-card", in: app).exists)
        XCTAssertTrue(played, "The moments did not play to the newest.")
    }

    /// The rain radar's card in the dark appearance, over the map, which
    /// stays light: closed and with "Key and notes" open, the map closer
    /// in, over the south of Bavaria and Tirol, where a pixel of a
    /// picture is to be told from the next and the radars' picture gives
    /// way to the analysis south of the main ridge.
    func testTheRainRadarsCardInTheDark() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, dark: true, extra: ["--server", server, "--map-at", "47.2,11.6,7.4"])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("forecast-chip", on: false, in: app)
        setLayer("radar-chip", on: true, in: app)
        XCTAssertTrue(element("radar-frames", in: app).waitForExistence(timeout: 60))
        let shown = radarSays(", 4 minutes ago", in: app, timeout: 60)
        sleep(4)
        keepScreenshot("map-radar-dark-1-card")
        showRadarKeyAndNotes(true, in: app)
        let key = element("radar-key", in: app).waitForExistence(timeout: 15)
        sleep(1)
        keepScreenshot("map-radar-dark-2-key-and-notes")
        showRadarKeyAndNotes(false, in: app)
        setLayer("radar-chip", on: false, in: app)
        XCTAssertFalse(element("radar-chip", in: app).isSelected)
        XCTAssertTrue(shown, "The radar's line did not come.")
        XCTAssertTrue(key, "The radar's key did not open.")
    }

    /// The rain radar when Venti is not reached: the card says so, with
    /// the one thing to do about it.
    func testTheRainRadarSaysWhenVentiIsNotReached() {
        let app = launch(bundledMap: true)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("forecast-chip", on: false, in: app)
        setLayer("radar-chip", on: true, in: app)
        let problem = element("radar-problem", in: app)
        let said = problem.waitForExistence(timeout: 60)
        let words = said ? problem.label : ""
        let again = app.buttons["Try again"].exists
        sleep(1)
        keepScreenshot("map-radar-not-reached")
        setLayer("radar-chip", on: false, in: app)
        XCTAssertFalse(element("radar-chip", in: app).isSelected)
        XCTAssertTrue(words.hasPrefix("Venti could not be reached for the rain radar."), words)
        XCTAssertTrue(again)
    }

    /// The app's icon on the home screen: the design language's, which
    /// the screenshot is kept to show.
    func testTheAppHasItsIconOnTheHomeScreen() {
        let app = XCUIApplication()
        app.launchArguments = Self.naming(["--ui-testing"])
        app.launch()
        XCTAssertTrue(app.tabBars.firstMatch.waitForExistence(timeout: 60))
        XCUIDevice.shared.press(.home)

        // The icon is on whichever page of the home screen had room.
        let home = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let icon = home.icons["Venti"]
        XCTAssertTrue(icon.waitForExistence(timeout: 15))
        for _ in 0..<4 where !icon.isHittable {
            home.swipeLeft()
            sleep(1)
        }
        XCTAssertTrue(icon.isHittable)
        sleep(1)
        keepScreenshot("icon-1-home-screen")
    }

    /// Today when Venti is not reached: the app opens on it, says so and
    /// offers to try again. The settings name no address: the app talks to
    /// the Venti it was built with.
    func testTodayWithoutVentiSaysWhatIsMissing() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = Self.naming(["--ui-testing", "--bundled-map"])
        app.launch()

        XCTAssertTrue(app.staticTexts["The forecast could not be read"].waitForExistence(timeout: 90))
        XCTAssertTrue(app.buttons["Try again"].exists)
        XCTAssertTrue(element("today-headline", in: app).exists)
        sleep(1)
        keepScreenshot("today-0-not-connected")
        app.buttons["Settings"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 15))
        XCTAssertFalse(app.textFields["Server address"].exists)
        // The section "Venti", further down: the account and the maps.
        let accountRow = element("account-link", in: app)
        for _ in 0..<4 where !accountRow.exists {
            app.swipeUp()
        }
        XCTAssertTrue(accountRow.waitForExistence(timeout: 10))
        XCTAssertFalse(app.textFields["Server address"].exists)
        sleep(1)
        keepScreenshot("settings-0-venti")
        // The tabs stay while the settings are open.
        openTab("Fly", in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))
    }

    /// The assistant, with the provider played by a script: the real tool
    /// loop runs against the logbook, but no key and no network are used.
    func testTheAssistantAnswersFromTheLogbook() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = Self.naming(["--ui-testing", "--bundled-map", "--scripted-assistant"])
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

    /// Others in the air, from the Open Glider Network, beside Venti's own
    /// pilots: the development server's made-up aircraft over the
    /// Tegernsee (paragliders, a hang glider, a glider, a tow plane), drawn
    /// in the muted ink, and a tap on the paraglider at the map's middle
    /// opens its sheet with the core's line and the network's credit. Light, then dark. Nothing else lies
    /// over the place tapped: the wind, the airspace and the sites are
    /// off, and switched on again at the end.
    func testTheMapShowsOthersInTheAirFromTheGliderNetwork() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let quiet = ["layer-wind", "layer-sites"]
        for dark in [false, true] {
            let app = launch(
                bundledMap: true, dark: dark, extra: ["--server", server, "--map-at", "47.6642,11.7967,11"]
            )
            XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
            for layer in quiet {
                setLayer(layer, on: false, in: app)
            }
            setLayer("layer-airspace", on: false, in: app)
            setLayer("layer-pilots", on: true, in: app)

            // The count of the Pilots layer, in the core's words, and the
            // network's credit with the map's credits.
            let pilots = layersRow("Pilots in the air", in: app)
            XCTAssertTrue(pilots.waitForExistence(timeout: 60))
            let counted = NSPredicate(format: "value CONTAINS 'others in the air'")
            expectation(for: counted, evaluatedWith: pilots)
            waitForExpectations(timeout: 30)
            closeLayers(in: app)
            XCTAssertTrue(waitForCredit("Open Glider Network", in: app, timeout: 15))
            sleep(3)
            keepScreenshot(dark ? "others-3-map-dark" : "others-1-map")

            // The paraglider at the map's middle, which lies where the
            // map's insets put it, some 11 points above the window's. Not
            // the hang glider: the label of Venti's landed demo glider
            // lies beside it, and Venti's pilots are on top for a tap.
            let middle = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            let line = element("other-line", in: app)
            for nudge in [0.0, -8, 8] where !line.exists {
                middle.withOffset(CGVector(dx: 0, dy: -11 + nudge)).tap()
                _ = line.waitForExistence(timeout: 4)
            }
            XCTAssertTrue(line.waitForExistence(timeout: 5))
            XCTAssertEqual(line.label, "Paraglider at 2,140 m, climbing 1.8 m/s, 20 s ago")
            XCTAssertEqual(element("other-name", in: app).label, "WB, Paraglider")
            XCTAssertTrue(element("other-credit", in: app).exists)
            XCTAssertTrue(app.navigationBars["Paraglider"].exists)
            sleep(1)
            keepScreenshot(dark ? "others-4-sheet-dark" : "others-2-sheet")
            app.buttons["Done"].tap()
            XCTAssertTrue(app.buttons["map-layers"].waitForExistence(timeout: 15))
            for layer in quiet {
                setLayer(layer, on: true, in: app)
            }
            app.terminate()
        }
    }

    /// The map stays on a pilot one follows: "Follow on the map" on the
    /// glider's page of the development server's demo glider in the air,
    /// on and selected after a tap; a drag of the map by hand switches it
    /// off. Light, then dark. The demo glider stands still, so no test
    /// sees the map go after a new position.
    func testTheMapStaysOnAPilotOneFollows() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        for dark in [false, true] {
            let app = launch(bundledMap: true, dark: dark, extra: ["--server", server])
            XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
            setLayer("layer-pilots", on: true, in: app)
            let pilots = layersRow("Pilots in the air", in: app, timeout: 30)
            XCTAssertTrue(pilots.exists)
            pilots.tap()
            XCTAssertTrue(app.navigationBars["In the air"].waitForExistence(timeout: 15))
            // The glider in the air, not the one that landed.
            let flying = app.buttons
                .matching(NSPredicate(format: "identifier == 'live-row' AND NOT (label CONTAINS 'Landed')"))
                .firstMatch
            XCTAssertTrue(flying.waitForExistence(timeout: 15))
            flying.tap()

            let follow = app.buttons["live-keep-on-map"]
            XCTAssertTrue(follow.waitForExistence(timeout: 20))
            XCTAssertEqual(follow.label, "Follow on the map")
            XCTAssertFalse(follow.isSelected)
            // The page moves the map to the flight first.
            sleep(3)
            follow.tap()
            XCTAssertTrue(follow.isSelected)
            sleep(3)
            keepScreenshot(dark ? "live-follow-dark-1-on" : "live-follow-1-on")

            // A drag of the map above the page lets it go.
            let from = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.22))
            from.press(forDuration: 0.1, thenDragTo: from.withOffset(CGVector(dx: 90, dy: 40)))
            let off = NSPredicate(format: "isSelected == false")
            expectation(for: off, evaluatedWith: follow)
            waitForExpectations(timeout: 10)
            sleep(1)
            keepScreenshot(dark ? "live-follow-dark-2-let-go" : "live-follow-2-let-go")

            // On again, and the page closed: off when it opens again.
            follow.tap()
            XCTAssertTrue(follow.isSelected)
            app.buttons["Done"].tap()
            XCTAssertTrue(app.buttons["map-layers"].waitForExistence(timeout: 15))
            layersRow("Pilots in the air", in: app, timeout: 30).tap()
            XCTAssertTrue(flying.waitForExistence(timeout: 15))
            flying.tap()
            XCTAssertTrue(follow.waitForExistence(timeout: 20))
            XCTAssertFalse(follow.isSelected)
            app.buttons["Done"].tap()
            app.terminate()
        }
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
        openTab("Map", in: app)

        XCTAssertTrue(app.buttons["map-layers"].waitForExistence(timeout: 90))
        // Every layer on: the choices are kept between launches, and a
        // test that stopped half-way may have left one off.
        for layer in ["layer-pilots", "layer-sites"] {
            setLayer(layer, on: true, in: app)
        }
        setLayer("layer-airspace", on: false, in: app)
        // The credit appears with the first answer from the server.
        XCTAssertTrue(waitForCredit("Wind", in: app))
        sleep(12)
        keepScreenshot("wind-1-map")
        // The layers sheet: the layers without a chip, and the lists.
        let stations = layersRow("Wind stations", in: app)
        XCTAssertTrue(stations.exists)
        sleep(1)
        keepScreenshot("wind-1a-layers")
        closeLayers(in: app)
        // The credits in full, one tap from the map.
        app.buttons["map-credits"].tap()
        XCTAssertTrue(element("wind-credits", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("wind-1b-credits")
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)

        layersRow("Wind stations", in: app).tap()
        XCTAssertTrue(app.navigationBars["Wind stations"].waitForExistence(timeout: 15))
        let summit = labelled(containing: "Zugspitze", in: app)
        XCTAssertTrue(summit.waitForExistence(timeout: 15))
        // A name the provider writes in capitals is shown in ordinary case.
        XCTAssertTrue(labelled(containing: "Patscherkofel", in: app).exists)
        sleep(1)
        keepScreenshot("wind-2-stations")

        summit.tap()
        XCTAssertTrue(element("wind-chart", in: app).waitForExistence(timeout: 30))
        // Whose wind it is: the station's own source stands at the page's
        // foot, below what the list has built; the map under the sheet
        // names it behind its credits button.
        let credited = (app.buttons["map-credits"].value as? String) ?? ""
        XCTAssertTrue(credited.contains("Deutscher Wetterdienst"), credited)
        sleep(2)
        keepScreenshot("wind-3-station")

        // What the readings say of the air, where the core says something:
        // a cloud base only while the sun is up at the station, a step in
        // temperature only with a station higher or lower nearby. Kept as
        // a screenshot when there; not a condition.
        if element("station-cloud-base", in: app).exists || element("station-temperature-step", in: app).exists {
            keepScreenshot("station-1-air")
        }

        // A past day, chosen with the date picker: the day before.
        // The compact picker is one element, "Date Picker", whose value is
        // the date; the simulator's region writes it "18. Nov 2026".
        let picker = app.descendants(matching: .any).matching(NSPredicate(format: "label == 'Date Picker'")).firstMatch
        for _ in 0..<3 where !isOnScreen(picker, in: app) {
            app.swipeUp()
        }
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        picker.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        let dayBefore = app.buttons
            .matching(NSPredicate(format: "label CONTAINS '17' AND label CONTAINS 'November'"))
            .firstMatch
        XCTAssertTrue(dayBefore.waitForExistence(timeout: 10))
        dayBefore.tap()
        // The calendar stays open after the choice and closes with a tap
        // beside it: on the line that says how old the reading is.
        if dayBefore.exists {
            labelled(containing: "Measured", in: app).firstMatch.tap()
        }
        for _ in 0..<3 where dayBefore.exists {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.38)).tap()
            usleep(500_000)
        }
        let pastDay = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier IN %@", ["wind-chart", "station-day-empty"]))
            .firstMatch
        XCTAssertTrue(pastDay.waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Day", in: app).exists)
        sleep(2)
        keepScreenshot("station-2-past-day")

        // Back to the map.
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["map-layers"].waitForExistence(timeout: 15))

        // After the stations, because a glider's page moves the map to its
        // flight, and the list of stations is of the part of the map in view.
        // Who is in the air, from the same server: its development mode
        // has two gliders over the Tegernsee, one flying and one landed.
        let pilots = layersRow("Pilots in the air", in: app, timeout: 30)
        XCTAssertTrue(pilots.exists)
        pilots.tap()
        XCTAssertTrue(app.navigationBars["In the air"].waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Demo Pilot", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(saying("Landed", in: app).exists)
        sleep(1)
        keepScreenshot("wind-1b-in-the-air")
        // Choosing one closes the list and opens the glider's page over
        // the map, which draws the flight so far and moves to show it.
        element("live-row", in: app).tap()
        XCTAssertTrue(element("live-age", in: app).waitForExistence(timeout: 20))
        sleep(5)
        keepScreenshot("wind-1c-glider")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["map-layers"].waitForExistence(timeout: 15))

        // The assistant looks up the same wind on the same server.
        openTab("Assistant", in: app)
        let question = app.buttons["How is the wind on the Zugspitze right now?"]
        XCTAssertTrue(question.waitForExistence(timeout: 15))
        question.tap()
        XCTAssertTrue(labelled(containing: "Zugspitze: 54 km/h", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "the wind stations around a place", in: app).exists)
        sleep(1)
        keepScreenshot("wind-4-assistant")

        // What the pilots of the server share with everyone: on the
        // League tab. The server's development mode has public flights
        // of several made-up pilots.
        openTab("League", in: app)
        let everyone = app.buttons["Everyone"]
        XCTAssertTrue(everyone.waitForExistence(timeout: 15))
        everyone.tap()
        let flight = element("public-flight-row", in: app)
        XCTAssertTrue(flight.waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Mara Muster", in: app).exists)
        sleep(1)
        keepScreenshot("community-1-everyone")
        // The pilots of the server by their best flights. The development
        // mode's flights are of 2025 and 2026; every year together holds
        // them whatever year the test runs in.
        app.buttons["Ranking"].tap()
        let allYears = app.buttons["All years"]
        XCTAssertTrue(allYears.waitForExistence(timeout: 30))
        allYears.tap()
        XCTAssertTrue(element("ranked-pilot", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Mara Muster", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("community-1b-ranking")
        // The rule stands under the pilots: with the development server's
        // Lena Probe among them it is below the screen's end.
        let rule = labelled(containing: "best 3 flights shared with everyone", in: app)
        for _ in 0..<4 where !rule.exists {
            app.swipeUp()
        }
        XCTAssertTrue(rule.waitForExistence(timeout: 5))
        for _ in 0..<4 where !everyone.isHittable {
            app.swipeDown()
        }
        everyone.tap()
        XCTAssertTrue(flight.waitForExistence(timeout: 30))
        // Another pilot's flight opens like one's own.
        flight.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 40))
        sleep(6)
        keepScreenshot("community-2-flight")
        // From the flight to everything its pilot shares.
        app.buttons["More"].tap()
        let ofPilot = labelled(containing: "Flights of", in: app)
        XCTAssertTrue(ofPilot.waitForExistence(timeout: 10))
        ofPilot.tap()
        // The pilot's totals stand over the flights: the page has come
        // once a total is there, and the flights are below the screen's
        // end, where a list has not built them yet.
        XCTAssertTrue(element("pilot-total", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("community-3-pilot")
        let ofTheirs = element("pilot-flight-row", in: app)
        for _ in 0..<6 where !ofTheirs.exists {
            app.swipeUp()
        }
        XCTAssertTrue(ofTheirs.waitForExistence(timeout: 10))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 20))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(everyone.waitForExistence(timeout: 15))
    }

    /// Pilots among each other, as the development server's demo pilot:
    /// what the pilot was told, under the account; the demo pilot's own
    /// flight from one of those lines, with who cheered and the two
    /// made-up comments, and no cheer button on one's own flight; Timo
    /// Entwurf's page with its counts and the button that follows him; the
    /// feed of the pilots the demo pilot follows, with the core's line of
    /// cheers and comments under a flight; and one of Lena Probe's flights
    /// cheered and written under. Then the feed in the dark. Skipped
    /// without a server.
    func testPilotsFollowCheerAndWriteUnderAFlight() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        // What the pilot was told: a row of the account, the newest line
        // on top.
        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        bring("account-notifications", into: app).tap()
        XCTAssertTrue(app.navigationBars["Notifications"].waitForExistence(timeout: 15))
        XCTAssertTrue(element("notification", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Lena Probe (@lena) follows you", in: app).exists)
        let commented = labelled(containing: "Mara Muster (@mara) commented on your flight", in: app)
        XCTAssertTrue(commented.exists)
        XCTAssertTrue(labelled(containing: "Lena Probe (@lena) cheered your flight", in: app).exists)
        sleep(1)
        keepScreenshot("social-1-notifications")

        // A line about a flight opens that flight: the pilot's own, so
        // without a cheer button.
        commented.tap()
        let comments = element("flight-comments", in: app)
        XCTAssertTrue(comments.waitForExistence(timeout: 40))
        XCTAssertEqual(comments.label, "2 comments")
        XCTAssertTrue(labelled(containing: "2 pilots cheered", in: app).waitForExistence(timeout: 15))
        XCTAssertFalse(element("flight-cheer", in: app).exists)
        sleep(6)
        keepScreenshot("social-2-own-flight")
        comments.tap()
        let count = element("comments-count", in: app)
        XCTAssertTrue(count.waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Well flown. How was the air above the ridge?", in: app).exists)
        XCTAssertTrue(labelled(containing: "Mara Muster · ", in: app).exists)
        // Under one's own flight every comment can be removed, and
        // another pilot's reported.
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "comment-remove").count, 2)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "comment-report").count, 1)
        XCTAssertTrue(element("comment-text", in: app).exists)
        sleep(1)
        keepScreenshot("social-3-comments")
        element("comments-close", in: app).tap()
        XCTAssertTrue(comments.waitForExistence(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Notifications"].waitForExistence(timeout: 15))

        // The feed: the flights of the pilots followed, each with what
        // pilots made of it.
        openTab("League", in: app)
        let following = app.buttons["Following"]
        XCTAssertTrue(following.waitForExistence(timeout: 15))
        following.tap()
        let rows = app.descendants(matching: .any).matching(identifier: "feed-row")
        XCTAssertTrue(rows.firstMatch.waitForExistence(timeout: 30))
        let maras = rows.matching(
            NSPredicate(format: "label CONTAINS 'Mara Muster' AND label CONTAINS 'You and 1 other cheered'")
        ).firstMatch
        for _ in 0..<4 where !maras.exists {
            app.swipeUp()
        }
        XCTAssertTrue(maras.waitForExistence(timeout: 10))
        XCTAssertTrue(maras.label.contains("1 comment"), maras.label)
        sleep(1)
        keepScreenshot("social-4-feed")

        // Another pilot's flight: cheered, and written under.
        let lenas = rows.matching(NSPredicate(format: "label CONTAINS 'Lena Probe'")).firstMatch
        for _ in 0..<4 where !isOnScreen(lenas, in: app) {
            app.swipeUp()
        }
        XCTAssertTrue(lenas.waitForExistence(timeout: 10))
        lenas.tap()
        let cheer = element("flight-cheer", in: app)
        XCTAssertTrue(cheer.waitForExistence(timeout: 40))
        if cheer.label != "Cheered" {
            cheer.tap()
        }
        XCTAssertTrue(
            app.buttons.matching(NSPredicate(format: "identifier == 'flight-cheer' AND label == 'Cheered'"))
                .firstMatch.waitForExistence(timeout: 20)
        )
        // The pilot alone cheered: the button says it, and no line beside
        // it says it again.
        XCTAssertFalse(element("flight-cheers-line", in: app).exists)
        XCTAssertTrue(comments.waitForExistence(timeout: 15))
        comments.tap()
        let text = element("comment-text", in: app)
        XCTAssertTrue(text.waitForExistence(timeout: 15))
        text.tap()
        text.typeText("Nice line along the ridge.")
        XCTAssertTrue(saying("26 of 500", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(element("comment-notice", in: app).exists)
        element("comment-send", in: app).tap()
        // Its age goes by Venti's clock as read, advanced by the time
        // since: the development server's clock stands still, so the
        // comment it dates "now" is a minute or two old by then.
        let written = app.descendants(matching: .any).matching(NSPredicate(
            format: "label CONTAINS 'Demo Pilot · ' AND (label CONTAINS 'just now' OR label CONTAINS 'min ago')"
        )).firstMatch
        XCTAssertTrue(written.waitForExistence(timeout: 20))
        XCTAssertTrue(element("comment-remove", in: app).exists)
        XCTAssertFalse(element("comment-report", in: app).exists)
        sleep(1)
        keepScreenshot("social-5-written")
        element("comments-close", in: app).tap()
        XCTAssertTrue(comments.waitForExistence(timeout: 15))
        XCTAssertTrue(comments.label.contains("comment"), comments.label)
        sleep(4)
        keepScreenshot("social-6-cheered")

        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(following.waitForExistence(timeout: 15))

        // A pilot's page, from the field over everyone's flights: the
        // counts, and the button that follows them.
        app.buttons["Everyone"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 15))
        field.tap()
        field.typeText("timo")
        let timo = labelled(containing: "Timo Entwurf (@timo)", in: app)
        XCTAssertTrue(timo.waitForExistence(timeout: 30))
        timo.tap()
        let follow = element("pilot-follow", in: app)
        XCTAssertTrue(follow.waitForExistence(timeout: 30))
        let follows = element("pilot-follows", in: app)
        XCTAssertTrue(follows.label.contains("Follows"), follows.label)
        sleep(1)
        keepScreenshot("social-7-pilot")
        if follow.label == "Follow" {
            follow.tap()
        }
        XCTAssertTrue(
            app.buttons.matching(NSPredicate(format: "identifier == 'pilot-follow' AND label == 'Following'"))
                .firstMatch.waitForExistence(timeout: 20)
        )
        XCTAssertTrue(follows.label.contains("1 follower"), follows.label)
        sleep(1)
        keepScreenshot("social-8-following")

        // The feed in the dark. A second start is signed out.
        let dark = launch(bundledMap: true, dark: true, extra: ["--server", server])
        XCTAssertTrue(dark.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: dark)
        openTab("League", in: dark)
        let inTheDark = dark.buttons["Following"]
        XCTAssertTrue(inTheDark.waitForExistence(timeout: 15))
        inTheDark.tap()
        XCTAssertTrue(element("feed-row", in: dark).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("social-9-feed-dark")
    }

    /// A long press on the map shows the server's forecast for the place
    /// under the finger. Skipped without a server.
    func testALongPressOnTheMapShowsTheForecastThere() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        sleep(3)
        // The map opens on the northern Alps; the left of its upper half
        // lies inside the server's first region.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.35, dy: 0.4)).press(forDuration: 1.3)
        XCTAssertTrue(element("forecast-table", in: app).waitForExistence(timeout: 40))
        XCTAssertTrue(labelled(containing: "hPa", in: app).exists)
        sleep(1)
        keepScreenshot("map-forecast")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 10))
    }

    /// A task: the Fly tab says which one the cockpit will fly, its page
    /// shows it and hands it on as a QR code, and in a replay of the
    /// sample flight the cockpit counts down to the start, takes it and
    /// leads on to the next turnpoint.
    func testATaskLeadsThroughTheCockpit() {
        let app = launch(bundledMap: true, extra: ["--sample-task"])
        openTab("Fly", in: app)
        let row = element("task-before-takeoff", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        XCTAssertTrue(saying("4 turnpoints", in: app).exists)
        row.tap()
        XCTAssertTrue(app.navigationBars["Task"].waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Sample turnpoint west", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(element("task-sketch", in: app).exists)
        sleep(1)
        keepScreenshot("task-1-task")

        app.buttons["More"].tap()
        let showCode = app.buttons["Show as a QR code"]
        XCTAssertTrue(showCode.waitForExistence(timeout: 10))
        showCode.tap()
        XCTAssertTrue(element("task-code", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("task-2-code")
        app.buttons["Done"].tap()
        app.navigationBars.buttons.firstMatch.tap()

        // The flight leaves the start cylinder seven minutes in, which at
        // twenty times its speed is after some twenty seconds.
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))
        choose("20 times faster", from: app.buttons["Replay the sample flight"], in: app)
        // Where the flight stands in its task is on the map's page.
        nextPage(in: app)
        let strip = element("task-strip", in: app)
        XCTAssertTrue(strip.waitForExistence(timeout: 30))
        XCTAssertTrue(valued(containing: "Sample start", in: app).waitForExistence(timeout: 10))
        keepScreenshot("task-3-before-start")

        XCTAssertTrue(valued(containing: "On task", in: app).waitForExistence(timeout: 240))
        XCTAssertTrue(valued(containing: "Sample turnpoint west", in: app).waitForExistence(timeout: 10))
        app.buttons["Show the whole flight"].tap()
        XCTAssertTrue(app.buttons["Follow the glider"].waitForExistence(timeout: 10))
        sleep(2)
        keepScreenshot("task-4-on-task")

        // The whole task with what is reached, and the two buttons for
        // when the task and the flight have come apart.
        strip.tap()
        XCTAssertTrue(app.navigationBars["Task"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("task-turnpoint", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("task-5-progress")
        // The buttons are below the turnpoints, further down the sheet.
        app.swipeUp()
        XCTAssertTrue(app.buttons["Skip to the next turnpoint"].waitForExistence(timeout: 10))
        app.buttons["Done"].tap()

        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))
    }

    /// The task page of the cockpit, in a replay of the sample flight
    /// with the sample task: the next turnpoint's name and readings, and
    /// on the map's page the closing circle where the flight makes one.
    func testTheCockpitShowsTheTaskPage() {
        let app = launch(bundledMap: true, extra: ["--sample-task"])
        openTab("Fly", in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 30))
        choose("20 times faster", from: app.buttons["Replay the sample flight"], in: app)
        // Instruments, map, flight, then the task's page.
        for _ in 0..<3 {
            nextPage(in: app)
        }
        XCTAssertTrue(element("task-next", in: app).waitForExistence(timeout: 60))
        XCTAssertTrue(element("task-next-name", in: app).exists)
        XCTAssertTrue(element("task-next-over", in: app).exists)
        sleep(2)
        keepScreenshot("cockpit-task-1-page")
        // Later in the flight, on task.
        _ = element("task-start-words", in: app).waitForNonExistence(timeout: 240)
        sleep(2)
        keepScreenshot("cockpit-task-2-on-task")
        // The map's page, with the closing circle if the flight makes one.
        nextPage(in: app)
        nextPage(in: app)
        if element("cockpit-closing-words", in: app).waitForExistence(timeout: 60) {
            app.buttons["Show the whole flight"].tap()
            sleep(2)
        }
        keepScreenshot("cockpit-task-3-map")
        endTheCockpit(in: app)
    }

    /// Cables by the Tegernsee, which the development server has recorded:
    /// the Wallberg's gondola drawn on the map, and a tap on it gives its
    /// words, its credit and the core's sentence on what is not mapped.
    func testTheMapShowsCablesByTheTegernsee() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        for dark in [false, true] {
            let app = launch(
                bundledMap: true, dark: dark, extra: ["--server", server, "--map-at", "47.6696709,11.7834441,14.5"]
            )
            XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
            setLayer("layer-cables", on: true, in: app)
            XCTAssertTrue(waitForCredit("Cables: OpenStreetMap contributors", in: app))
            sleep(2)
            keepScreenshot(dark ? "map-cables-dark-1-drawn" : "map-cables-1-drawn")
            // The line passes through the middle of the map.
            let words = element("cable-words", in: app)
            for _ in 0..<5 where !words.exists {
                app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
                _ = words.waitForExistence(timeout: 3)
            }
            XCTAssertTrue(words.exists)
            XCTAssertTrue(words.label.contains("Gondola"), words.label)
            XCTAssertTrue(element("cable-limits", in: app).exists)
            sleep(1)
            keepScreenshot(dark ? "map-cables-dark-2-tapped" : "map-cables-2-tapped")
            app.buttons["Done"].tap()
            app.terminate()
        }
    }

    /// What others found in the air, against a Venti server in development
    /// mode, which has two made-up climbs by the Tegernsee and two cells
    /// where its made-up public flights climbed: the layers sheet's two
    /// switches, off until switched on; "Climbs now" with a mark for each
    /// climb and a climb's line on a tap; and where thermals have been.
    /// Both are switched off again at the end: the choice is kept.
    func testTheMapShowsWhereOthersClimbNow() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        // The map opens on the climb east of the Wallberg.
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.6675,11.8025,13.5"])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        // The map has the server's answer once the sites' credit is there:
        // the Wallberg's ramps are in view. No wind station is.
        XCTAssertTrue(waitForCredit("Sites", in: app))
        // Both start off, whatever a test before this one left.
        setLayer("layer-climbs", on: false, in: app)
        setLayer("layer-flown", on: false, in: app)

        XCTAssertTrue(openLayers(in: app))
        let climbs = app.switches["layer-climbs"]
        XCTAssertTrue(climbs.waitForExistence(timeout: 10))
        XCTAssertTrue(app.switches["layer-flown"].exists)
        XCTAssertTrue(labelled(containing: "Climbs now", in: app).exists)
        XCTAssertTrue(labelled(containing: "Where thermals have been", in: app).exists)
        sleep(1)
        keepScreenshot("air-1-layers-off")
        set(climbs, on: true)
        // The layer says how many climb in the part of the map in view,
        // in the core's words: "1 climb in the last 10 minutes".
        let counted = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS %@ OR label CONTAINS %@", "climb in the last", "climbs in the last")
        ).firstMatch
        XCTAssertTrue(counted.waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("air-2-layers-climbs-on")
        closeLayers(in: app)
        sleep(2)
        keepScreenshot("air-3-map-climbs")

        // A tap on the climb in the map's middle, with what lies over it
        // off: its line, and whose gliders it is from.
        setLayer("layer-pilots", on: false, in: app)
        setLayer("layer-wind", on: false, in: app)
        setLayer("layer-sites", on: false, in: app)
        // The network's credit is with the map's while the climbs are drawn.
        XCTAssertTrue(waitForCredit("Open Glider Network", in: app, timeout: 20))
        let middle = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        let line = element("climb-line", in: app)
        for _ in 0..<5 where !line.exists {
            middle.tap()
            _ = line.waitForExistence(timeout: 3)
        }
        XCTAssertTrue(line.exists)
        XCTAssertTrue(line.label.hasPrefix("Climbing now: +2.4 m/s"), line.label)
        XCTAssertTrue(labelled(containing: "Open Glider Network", in: app).exists)
        sleep(1)
        keepScreenshot("air-4-climb-line")
        element("air-line-close", in: app).tap()
        usleep(800_000)

        // Where thermals have been: its cells on the map, and a cell's
        // line where the tap finds the one up and left of the middle.
        setLayer("layer-climbs", on: false, in: app)
        XCTAssertTrue(openLayers(in: app))
        let flown = app.switches["layer-flown"]
        XCTAssertTrue(flown.waitForExistence(timeout: 10))
        set(flown, on: true)
        closeLayers(in: app)
        sleep(4)
        keepScreenshot("air-5-map-flown")
        let cell = element("flown-line", in: app)
        middle.withOffset(CGVector(dx: -165, dy: -122)).tap()
        if cell.waitForExistence(timeout: 5) {
            sleep(1)
            keepScreenshot("air-6-flown-line")
        }
        // Whatever the tap opened is closed again.
        for close in [element("air-line-close", in: app), app.buttons["Done"].firstMatch] where close.exists {
            close.tap()
            usleep(800_000)
            break
        }

        // As it was: the two layers off, the others on.
        setLayer("layer-flown", on: false, in: app)
        setLayer("layer-pilots", on: true, in: app)
        setLayer("layer-wind", on: true, in: app)
        setLayer("layer-sites", on: true, in: app)
    }

    /// A site's page says who flies there now, in the core's one line: the
    /// development server's made-up gliders are in the air over the
    /// Wallberg, whose two ramps have no name of their own, so the line
    /// says "here".
    func testASitesPageSaysWhoIsInTheAirThere() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(
            bundledMap: true, extra: ["--server", server, "--all-sites", "--map-at", "47.6652,11.7954,13"]
        )
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        XCTAssertTrue(waitForCredit("Sites", in: app))
        layersRow("Flying sites", in: app).tap()
        let ramp = app.descendants(matching: .any).matching(identifier: "site-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Take-off", "47.665"))
            .firstMatch
        XCTAssertTrue(ramp.waitForExistence(timeout: 30))
        bringUp(ramp, in: app)
        ramp.tap()
        let line = element("site-air", in: app)
        XCTAssertTrue(line.waitForExistence(timeout: 30))
        XCTAssertTrue(line.label.contains("in the air here"), line.label)
        sleep(1)
        keepScreenshot("air-7-site-pilots-in-the-air")
    }

    /// The text size above which the larger-text tests look: the largest
    /// of the standard ones, as the forecast card's test has it.
    private static let largerText = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL"]

    /// The map tab's new sheets in the dark and with larger text: the
    /// layers sheet with the climbs, the map with them, the credits, a
    /// site's page with the pilots in the air and its fold row, and the
    /// form of a new alarm for the site with its kinds. Run it alone on
    /// the narrowest simulator (`--first "iPhone 17e"`).
    func testTheMapsNewSheetsInTheDarkWithLargerText() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(
            bundledMap: true, dark: true,
            extra: ["--server", server, "--all-sites", "--map-at", "47.6652,11.7954,13"] + Self.largerText
        )
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)
        XCTAssertTrue(waitForCredit("Sites", in: app))
        setLayer("layer-flown", on: false, in: app)

        XCTAssertTrue(openLayers(in: app))
        let climbs = app.switches["layer-climbs"]
        XCTAssertTrue(climbs.waitForExistence(timeout: 10))
        set(climbs, on: true)
        sleep(2)
        keepScreenshot("larger-dark-1-layers")
        app.swipeUp()
        sleep(1)
        keepScreenshot("larger-dark-1b-layers-lower")
        closeLayers(in: app)
        sleep(2)
        keepScreenshot("larger-dark-2-map-climbs")

        app.buttons["map-credits"].tap()
        XCTAssertTrue(app.navigationBars["Credits"].waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("larger-dark-3-credits")
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)
        setLayer("layer-climbs", on: false, in: app)

        layersRow("Flying sites", in: app).tap()
        let ramp = app.descendants(matching: .any).matching(identifier: "site-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Take-off", "47.665"))
            .firstMatch
        XCTAssertTrue(ramp.waitForExistence(timeout: 30))
        bringUp(ramp, in: app)
        ramp.tap()
        XCTAssertTrue(element("site-air", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("larger-dark-4-site-pilots-in-the-air")
        let fold = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "How this is read")).firstMatch
        for _ in 0..<10 where !isOnScreen(fold, in: app) {
            drawUp(in: app)
        }
        XCTAssertTrue(fold.exists)
        sleep(1)
        keepScreenshot("larger-dark-5-site-fold-row")

        bring("alert-me", into: app, down: true).tap()
        XCTAssertTrue(element("alert-kind", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("larger-dark-6-alert-kinds")
    }

    /// What pilots see of each other, in the dark and with larger text:
    /// the notifications, a flight with its cheers and comments, the
    /// comments, the feed and a pilot's page with "Follow". It changes
    /// nothing on the server.
    func testThePilotsScreensInTheDarkWithLargerText() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, dark: true, extra: ["--server", server] + Self.largerText)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        bring("account-notifications", into: app).tap()
        XCTAssertTrue(app.navigationBars["Notifications"].waitForExistence(timeout: 15))
        XCTAssertTrue(element("notification", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("larger-dark-social-1-notifications")

        labelled(containing: "commented on your flight", in: app).tap()
        let comments = element("flight-comments", in: app)
        XCTAssertTrue(comments.waitForExistence(timeout: 40))
        sleep(6)
        keepScreenshot("larger-dark-social-2-flight-pills")
        comments.tap()
        XCTAssertTrue(element("comments-count", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("larger-dark-social-3-comments")
        element("comments-close", in: app).tap()
        XCTAssertTrue(comments.waitForExistence(timeout: 15))
        // A flight's page covers the tab bar: back to the notifications.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Notifications"].waitForExistence(timeout: 15))

        openTab("League", in: app)
        let following = app.buttons["Following"]
        XCTAssertTrue(following.waitForExistence(timeout: 15))
        following.tap()
        XCTAssertTrue(element("feed-row", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("larger-dark-social-4-feed")

        app.buttons["Everyone"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 15))
        field.tap()
        field.typeText("timo")
        let timo = labelled(containing: "Timo Entwurf (@timo)", in: app)
        XCTAssertTrue(timo.waitForExistence(timeout: 30))
        timo.tap()
        XCTAssertTrue(element("pilot-follow", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("larger-dark-social-5-pilot")
    }

    /// "Glider and cockpit" in the dark with larger text, and the
    /// cockpit's details sheet with larger text: the cockpit is in the
    /// flight theme, light whatever the iPhone is set to.
    func testTheCockpitsSettingsAndDetailsWithLargerText() {
        let app = launch(bundledMap: true, dark: true, extra: ["--sample-waypoints"] + Self.largerText)
        openTab("Settings", in: app)
        let cockpit = element("cockpit-settings-link", in: app)
        for _ in 0..<4 where !(cockpit.exists && cockpit.isHittable) {
            app.swipeUp()
        }
        cockpit.tap()
        XCTAssertTrue(element("glider-class", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("larger-dark-cockpit-1-glider-and-cockpit")
        app.swipeUp()
        sleep(1)
        keepScreenshot("larger-dark-cockpit-2-glider-and-cockpit-lower")
        app.navigationBars.buttons.firstMatch.tap()

        openTab("Fly", in: app)
        let replay = app.buttons["Replay the sample flight"]
        XCTAssertTrue(replay.waitForExistence(timeout: 30))
        choose("20 times faster", from: replay, in: app)
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Map"))
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Flight"))
        element("cockpit-details", in: app).tap()
        XCTAssertTrue(app.navigationBars["Details"].waitForExistence(timeout: 10))
        XCTAssertTrue(element("cockpit-detail", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("larger-cockpit-3-details")
        app.navigationBars["Details"].buttons["Done"].tap()
        endTheCockpit(in: app)
    }

    /// A route of the pilot's own waypoints: tapped in the order to fly
    /// them and kept as the task.
    func testARouteIsPlannedFromWaypoints() {
        let app = launch(bundledMap: true, extra: ["--sample-waypoints"])
        openTab("Fly", in: app)
        let row = element("task-before-takeoff", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        let plan = app.buttons["Plan a route from your waypoints"]
        XCTAssertTrue(plan.waitForExistence(timeout: 15))
        plan.tap()
        XCTAssertTrue(app.navigationBars["Plan a route"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["Keep"].isEnabled)

        labelled(containing: "Sample landing field", in: app).tap()
        labelled(containing: "Sample turnpoint", in: app).tap()
        XCTAssertTrue(app.buttons["Keep"].isEnabled)
        sleep(1)
        keepScreenshot("route-1-plan")

        app.buttons["Keep"].tap()
        XCTAssertTrue(labelled(containing: "its waypoints", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(element("task-sketch", in: app).exists)
        sleep(1)
        keepScreenshot("route-2-kept")

        // Without waypoints: a route drawn on the map, a tap for each
        // place. Three places and back to the first make a triangle, and
        // the page says what it would score.
        let draw = element("task-draw-route", in: app)
        for _ in 0..<4 where !(draw.exists && draw.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(draw.waitForExistence(timeout: 10))
        draw.tap()
        let map = element("route-map", in: app)
        XCTAssertTrue(map.waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "Tap the map where the route begins", in: app).waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["Keep"].isEnabled)
        // Time for the map to draw before it is tapped.
        sleep(4)
        for (x, y) in [(0.3, 0.45), (0.7, 0.45), (0.5, 0.2)] {
            map.coordinate(withNormalizedOffset: CGVector(dx: x, dy: y)).tap()
            // A single tap waits for the map's double tap to be ruled out.
            sleep(1)
        }
        XCTAssertTrue(labelled(containing: "along the route", in: app).waitForExistence(timeout: 10))
        app.buttons["Back to the start"].tap()
        XCTAssertTrue(labelled(containing: "triangle", in: app).waitForExistence(timeout: 10))
        sleep(2)
        keepScreenshot("route-3-drawn")

        app.buttons["Keep"].tap()
        XCTAssertTrue(labelled(containing: "Turnpoint 1", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("route-4-drawn-kept")
    }

    /// The weather along a route drawn near the Tegernsee, the
    /// development mode's forecast region and the ground of the map that
    /// comes with the app: the route in words, each place's line and the
    /// height to leave it at, in light and once in the dark. Skipped
    /// without a server.
    func testARouteShowsTheWeatherAlongIt() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        for dark in [false, true] {
            let app = launch(
                bundledMap: true, dark: dark, extra: ["--server", server, "--map-at", "47.68,11.75,11"]
            )
            drawATriangle(in: app)
            let toggle = element("route-weather-toggle", in: app)
            XCTAssertTrue(toggle.waitForExistence(timeout: 10))
            XCTAssertEqual(toggle.value as? String, "Hidden")
            toggle.tap()
            XCTAssertTrue(element("route-weather-words", in: app).waitForExistence(timeout: 30))
            // The forecast's model and credits once Venti has answered.
            _ = element("route-weather-credits", in: app).waitForExistence(timeout: 30)
            let departure = element("route-weather-departure", in: app)
            XCTAssertTrue(departure.waitForExistence(timeout: 30))
            // The heights follow the forecast, once the ground is read.
            for _ in 0..<30 where !departure.label.contains(" m ") {
                sleep(1)
            }
            XCTAssertTrue(departure.label.contains(" m "), departure.label)
            XCTAssertGreaterThanOrEqual(app.descendants(matching: .any).matching(identifier: "route-weather-point").count, 3)
            sleep(2)
            keepScreenshot(dark ? "route-weather-3-dark" : "route-weather-1")
            if !dark {
                // Further down the card: the places, the legs and the credits.
                element("route-weather-words", in: app).swipeUp()
                sleep(1)
                keepScreenshot("route-weather-2-places")
            }
            app.terminate()
        }
    }

    /// What a glide reaches from the Wallberg: a long press on the map,
    /// "What a glide reaches from here" in what opens, and the area on the
    /// map with the core's words on the card under it, light and dark.
    func testAGlideReachesFromAPlacePressedOnTheMap() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        for dark in [false, true] {
            let app = launch(
                bundledMap: true, dark: dark, extra: ["--server", server, "--map-at", "47.6642,11.7967,12"]
            )
            XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
            // The development server's demo glider stands in the map's
            // middle, and the stations' arrows near it: off while pressing.
            setLayer("layer-pilots", on: false, in: app)
            setLayer("layer-wind", on: false, in: app)
            sleep(3)
            element("overview-map", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
                .press(forDuration: 1.3)
            let entry = element("glide-reach-here", in: app)
            XCTAssertTrue(entry.waitForExistence(timeout: 30))
            entry.tap()
            let words = element("glide-reach-words", in: app)
            XCTAssertTrue(words.waitForExistence(timeout: 60))
            XCTAssertTrue(words.label.hasPrefix("From"), words.label)
            XCTAssertTrue(element("glide-reach-note", in: app).exists)
            // A step of the glide asks the core again, and the card says so.
            if !dark {
                let before = words.label
                let named = app.buttons["glide-reach-glide-Increment"]
                (named.exists ? named : element("glide-reach-glide", in: app).buttons["Increment"]).tap()
                for _ in 0..<20 where words.label == before {
                    usleep(250_000)
                }
                XCTAssertNotEqual(words.label, before)
            }
            // A start 500 m over the ground, as from a launch higher up:
            // a wider area for the screenshots.
            let start = element("glide-reach-start", in: app)
            let higher = app.buttons["glide-reach-start-Increment"]
            for _ in 0..<10 {
                (higher.exists ? higher : start.buttons["Increment"]).tap()
                usleep(300_000)
            }
            XCTAssertTrue(words.label.hasPrefix("From"), words.label)
            // Time for the map to bring the area into view above the card.
            sleep(3)
            keepScreenshot(dark ? "glide-reach-2-dark" : "glide-reach-1")
            element("glide-reach-clear", in: app).tap()
            XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 10))
            setLayer("layer-pilots", on: true, in: app)
            setLayer("layer-wind", on: true, in: app)
            app.terminate()
        }
    }

    /// Opens the map to draw a route on from the Fly tab, and draws three
    /// places on it.
    private func drawATriangle(in app: XCUIApplication) {
        openTab("Fly", in: app)
        let row = element("task-before-takeoff", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        let draw = element("task-draw-route", in: app)
        for _ in 0..<4 where !(draw.exists && draw.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(draw.waitForExistence(timeout: 10))
        draw.tap()
        let map = element("route-map", in: app)
        XCTAssertTrue(map.waitForExistence(timeout: 20))
        // Time for the map to draw before it is tapped.
        sleep(4)
        for (x, y) in [(0.3, 0.35), (0.7, 0.35), (0.5, 0.15)] {
            map.coordinate(withNormalizedOffset: CGVector(dx: x, dy: y)).tap()
            // A single tap waits for the map's double tap to be ruled out.
            sleep(1)
        }
        XCTAssertTrue(labelled(containing: "along the route", in: app).waitForExistence(timeout: 10))
    }

    /// Airspace from the pilot's server: with a server set, the app has
    /// fetched the airspace around the map by itself. It says before
    /// take-off what the cockpit will warn from, and lists it with its
    /// provider's credit and notice. Skipped without a server.
    func testAirspaceComesFromTheServerByItself() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        // As on a new iPhone: Venti does not know where the pilot is until
        // the pilot says it may, further down.
        XCUIApplication().resetAuthorizationStatus(for: .location)
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        // The map tab's airspace is off from here to the end: what the
        // cockpit warns from must not depend on what that map draws.
        setAirspace(on: false, in: app)

        // The same server knows the flying sites. They are drawn once the
        // map shows a valley and not a country: with the sample flight in
        // the logbook the map shows the valley it was flown in.
        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))
        openTab("Map", in: app)
        XCTAssertTrue(waitForCredit("Sites", in: app))
        sleep(4)
        keepScreenshot("sites-1-map")

        // The signs on the map, as a list, and one of them.
        layersRow("Flying sites", in: app).tap()
        XCTAssertTrue(app.navigationBars["Flying sites"].waitForExistence(timeout: 15))
        let row = element("site-row", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("sites-2-list")
        row.tap()
        // The page is known by the button at its head: what is further
        // down depends on how much the forecast has to show of the day.
        XCTAssertTrue(app.buttons["Plan route"].waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("sites-3-site")

        // A route that begins here: the map to draw it on opens with the
        // site as its first place.
        app.buttons["Plan route"].tap()
        XCTAssertTrue(element("route-map", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(app.navigationBars["Draw a route"].exists)
        // The route can be drawn on photographs from above as well.
        XCTAssertTrue(app.buttons["map-aerial"].exists)
        sleep(3)
        keepScreenshot("sites-3a-plan-route")
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.buttons["Plan route"].waitForExistence(timeout: 15))

        // The same server's forecast for the place: the wind at each
        // height through the day, and the sky.
        // The route's map may still be on its way out over the page: the
        // row is tapped once it holds still and can be touched.
        // The row stands under the picture of the day's air, where the
        // forecast has one: the page is moved up until it is there.
        let forecast = element("site-forecast", in: app)
        for _ in 0..<4 where !forecast.exists {
            app.swipeUp()
        }
        tapWhenStill(forecast, timeout: 15)
        XCTAssertTrue(element("forecast-table", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "hPa", in: app).exists)
        sleep(1)
        keepScreenshot("sites-4-forecast")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(forecast.waitForExistence(timeout: 10))

        // Further down the site's page: where the site is from.
        let source = saying("OpenStreetMap", in: app)
        for _ in 0..<4 where !source.exists {
            app.swipeUp()
        }
        XCTAssertTrue(source.exists)

        // Back to the list and out of it. A tap while the page is still
        // settling from the swipes is taken for the end of the scroll and
        // goes nowhere: once more, then.
        app.navigationBars.buttons.firstMatch.tap()
        let done = app.buttons["Done"]
        if !done.waitForExistence(timeout: 5) {
            app.navigationBars.buttons.firstMatch.tap()
        }
        XCTAssertTrue(done.waitForExistence(timeout: 10))
        done.tap()

        // Nothing has asked for airspace so far: the map tab's is off, and
        // the Fly tab does not know where the pilot is.
        openTab("Fly", in: app)
        let line = element("airspace-before-takeoff", in: app)
        XCTAssertTrue(line.waitForExistence(timeout: 30))
        sleep(3)
        XCTAssertFalse(saying("openAIP", in: app).exists, "Airspace was fetched before the pilot's position was known.")

        // The pilot is by the Tegernsee and lets Venti know, on Today.
        XCUIDevice.shared.location = XCUILocation(
            location: CLLocation(latitude: 47.664, longitude: 11.772)
        )
        openTab("Today", in: app)
        // The button is at the page's foot, under the take-offs, and a row
        // that is not on screen does not exist yet.
        XCTAssertTrue(element("today-card", in: app).waitForExistence(timeout: 60))
        let mine = app.buttons["Use my position"]
        for _ in 0..<12 where !(mine.exists && isOnScreen(mine, in: app)) {
            app.swipeUp()
        }
        XCTAssertTrue(mine.exists)
        // Clear of the tab bar, which lies over the page's foot.
        if mine.frame.maxY > app.windows.firstMatch.frame.maxY - 120 {
            app.swipeUp()
        }
        mine.tap()
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow While Using App"]
        XCTAssertTrue(allow.waitForExistence(timeout: 15))
        allow.tap()

        // The Fly tab asks for the airspace around the pilot by itself and
        // says what the cockpit will warn from.
        openTab("Fly", in: app)
        XCTAssertTrue(line.waitForExistence(timeout: 30))
        XCTAssertTrue(saying("openAIP", in: app).waitForExistence(timeout: 90))
        keepScreenshot("airspace-1-before-takeoff")

        line.tap()
        XCTAssertTrue(element("airspace-from-server", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(saying("not certified", in: app).exists)
        keepScreenshot("airspace-2-from-server")
        app.navigationBars.buttons.firstMatch.tap()

        // The cockpit, waiting for the take-off, has that airspace to warn
        // from: the reading of the airspace that matters next stands on
        // its map page only while the session checks airspace, and the
        // map names whose it is.
        let open = app.buttons["Open the cockpit"]
        XCTAssertTrue(open.waitForExistence(timeout: 15))
        open.tap()
        XCTAssertTrue(element("cockpit-end", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(element("cockpit-airspace-credit", in: app).waitForExistence(timeout: 30))
        // Standing at the take-off, with positions that have a height:
        // the cockpit checks them against that airspace, and its notice
        // that nothing is checked yet goes.
        let unchecked = element("airspace-notice", in: app)
        for _ in 0..<20 {
            put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
            sleep(1)
            if !unchecked.exists {
                break
            }
        }
        XCTAssertFalse(unchecked.exists, "The cockpit does not check airspace while it waits for the take-off.")
        nextPage(in: app)
        XCTAssertTrue(element("airspace-proximity-map", in: app).waitForExistence(timeout: 30))
        put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
        sleep(1)
        keepScreenshot("airspace-3-cockpit-before-takeoff")
        endTheCockpit(in: app)

        // And the map tab's airspace was off all the while.
        openTab("Map", in: app)
        let button = app.buttons["map-airspace"]
        XCTAssertTrue(button.waitForExistence(timeout: 30))
        XCTAssertFalse(button.isSelected)
        // The simulator remembers that Venti may use the position, and
        // Today then looks around the pilot in the tests after this one.
        app.terminate()
        app.resetAuthorizationStatus(for: .location)
    }

    /// An account on a Venti server: signing in as the development
    /// server's demo pilot brings that pilot's three flights into the
    /// logbook, and one of them is shared with a link. Skipped without a
    /// server, like the wind test.
    func testAPilotSignsInAndSharesAFlight() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--sample-waypoints"])
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
        let handle = openThePasswordForm(in: app)
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
        // What the demo pilot had chosen for it, to be put back below: the
        // development server's notifications and feed name this flight as
        // a public one, and the tests after this one on the same server
        // open it from there.
        let chosenChoice = { (key: String) in
            app.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier == %@ AND value CONTAINS 'Chosen'", "share-\(key)"))
                .firstMatch
        }
        sleep(2)
        let before = ["public", "private"].first { chosenChoice($0).exists }
        withTheLink.tap()
        let chosen = chosenChoice("unlisted")
        XCTAssertTrue(chosen.waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "/f/", in: app).exists)
        sleep(1)
        keepScreenshot("account-4-share")
        if let before {
            element("share-\(before)", in: app).tap()
            XCTAssertTrue(chosenChoice(before).waitForExistence(timeout: 20))
        }

        // Something another pilot shares that should not be there is
        // reported to whoever runs the server: from the flight's menu, with
        // a reason, from the account signed in above. The server's
        // development mode takes the report and keeps it nowhere.
        app.buttons["Done"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        openTab("League", in: app)
        let everyone = app.buttons["Everyone"]
        XCTAssertTrue(everyone.waitForExistence(timeout: 15))
        everyone.tap()
        let theirs = element("public-flight-row", in: app)
        XCTAssertTrue(theirs.waitForExistence(timeout: 30))
        theirs.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 40))
        app.buttons["More"].tap()
        let report = app.buttons["Report this flight"]
        XCTAssertTrue(report.waitForExistence(timeout: 10))
        report.tap()
        let reason = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Not a real flight'")).firstMatch
        XCTAssertTrue(reason.waitForExistence(timeout: 10))
        // Nothing can be sent before a reason is chosen.
        XCTAssertFalse(app.buttons["Send"].isEnabled)
        reason.tap()
        sleep(1)
        keepScreenshot("community-4-report")
        app.buttons["Send"].tap()
        XCTAssertTrue(element("report-outcome", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "Whoever runs Venti will read it", in: app).exists)
        sleep(1)
        keepScreenshot("community-5-reported")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 15))

        // Who flies where: the plans of the pilot's groups, from the map.
        // The development mode's demo pilot keeps a made-up group.
        app.navigationBars.buttons.firstMatch.tap()
        openTab("Map", in: app)
        let plans = layersRow("Flying plans", in: app, timeout: 15)
        XCTAssertTrue(plans.exists)
        plans.tap()
        XCTAssertTrue(element("plans-groups", in: app).waitForExistence(timeout: 30))

        // A plan for today, posted to the group. Its place is taken from
        // the pilot's own waypoints, which know where they are, so the
        // plan can be marked on the map.
        app.buttons["New plan"].tap()
        XCTAssertTrue(element("plan-place", in: app).waitForExistence(timeout: 10))
        choose("Sample landing field", from: app.buttons["From my waypoints"], in: app)
        XCTAssertTrue(labelled(containing: "On the map", in: app).waitForExistence(timeout: 10))
        app.buttons["Save"].tap()
        let posted = labelled(containing: "Sample landing field", in: app)
        XCTAssertTrue(posted.waitForExistence(timeout: 20))
        XCTAssertTrue(app.staticTexts["Today"].exists || labelled(containing: "Today", in: app).exists)
        sleep(1)
        keepScreenshot("plans-1-board")

        // The plan itself: its author is in from the start, and can step
        // out again.
        posted.tap()
        XCTAssertTrue(element("plan-in", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "I am out", in: app).exists)
        sleep(1)
        keepScreenshot("plans-2-plan")
        app.navigationBars.buttons.firstMatch.tap()

        // Another member's plan that should not be there is reported as a
        // flight is: the development mode's "Beispielwiese" is mara's.
        // One's own plan has no such entry.
        // The board lists plans from today by this iPhone's clock, and the
        // development mode's are for 18 to 20 November: whether one of
        // another member's is there depends on the day the test runs.
        // Checked when there.
        let theirPlan = labelled(containing: "Beispielwiese", in: app)
        if theirPlan.waitForExistence(timeout: 5) {
            theirPlan.tap()
            let reportPlan = element("plan-report", in: app)
            XCTAssertTrue(reportPlan.waitForExistence(timeout: 10))
            reportPlan.tap()
            XCTAssertTrue(element("report-reason", in: app).waitForExistence(timeout: 10))
            XCTAssertTrue(app.navigationBars["Report this plan"].exists)
            sleep(1)
            keepScreenshot("groups-plan-report")
            app.buttons["Cancel"].tap()
            XCTAssertTrue(reportPlan.waitForExistence(timeout: 10))
            app.navigationBars.buttons.firstMatch.tap()
        }
        // One's own plan has no report entry.
        posted.tap()
        XCTAssertTrue(element("plan-in", in: app).waitForExistence(timeout: 10))
        XCTAssertFalse(element("plan-report", in: app).exists)
        app.navigationBars.buttons.firstMatch.tap()

        // And a group the pilot is in and does not keep: the development
        // mode's other group, joined with its code. The keeper of a group
        // deletes it instead.
        let groupsLink = element("plans-groups", in: app)
        XCTAssertTrue(groupsLink.waitForExistence(timeout: 10))
        groupsLink.tap()
        let join = app.buttons["Join with a code"]
        XCTAssertTrue(join.waitForExistence(timeout: 10))
        join.tap()
        let code = app.alerts.textFields.firstMatch
        XCTAssertTrue(code.waitForExistence(timeout: 10))
        code.typeText("5555-5555-5555-5555")
        app.alerts.buttons["Join"].tap()
        let otherGroup = labelled(containing: "Beispielgruppe", in: app)
        XCTAssertTrue(otherGroup.waitForExistence(timeout: 20))
        otherGroup.tap()
        let reportGroup = element("group-report", in: app)
        for _ in 0..<3 where !reportGroup.exists {
            app.swipeUp()
        }
        XCTAssertTrue(reportGroup.waitForExistence(timeout: 15))
        reportGroup.tap()
        XCTAssertTrue(element("report-reason", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(app.navigationBars["Report this group"].exists)
        sleep(1)
        keepScreenshot("groups-group-report")
        app.buttons["Cancel"].tap()
        XCTAssertTrue(reportGroup.waitForExistence(timeout: 10))
        // Back to the board.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(join.waitForExistence(timeout: 10))
        app.navigationBars.buttons.firstMatch.tap()

        // The map marks the plan where it is, and says so.
        XCTAssertTrue(element("plans-groups", in: app).waitForExistence(timeout: 10))
        app.buttons["Done"].tap()
        let map = element("overview-map", in: app)
        XCTAssertTrue(map.waitForExistence(timeout: 15))
        var said = ""
        for _ in 0..<40 where !said.contains("marked") {
            usleep(500_000)
            said = map.value as? String ?? ""
        }
        // One plan or more: the development mode's own plans are for
        // fixed days, which may be today or long past.
        XCTAssertTrue(said.contains("marked"), "The map says \"\(said)\"")
        sleep(2)
        keepScreenshot("plans-4-map")
        layersRow("Flying plans", in: app).tap()

        // The groups, and one of them with its members and what its
        // keeper can do.
        let toGroups = element("plans-groups", in: app)
        XCTAssertTrue(toGroups.waitForExistence(timeout: 10))
        toGroups.tap()
        let group = element("group-row", in: app)
        XCTAssertTrue(group.waitForExistence(timeout: 15))
        group.tap()
        XCTAssertTrue(element("group-member", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(app.buttons["New invite code"].exists)
        sleep(1)
        keepScreenshot("plans-3-group")
    }

    /// A flight is laid beside another pilot's of the same day: both on
    /// the map and in the profile, and their numbers side by side. Runs
    /// against a Venti server in development mode, where the demo
    /// pilot's one public flight and one of Mara's were flown from the
    /// same place on the same day.
    func testAFlightIsLaidBesideAnother() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        openTab("League", in: app)
        let everyone = app.buttons["Everyone"]
        XCTAssertTrue(everyone.waitForExistence(timeout: 15))
        everyone.tap()
        // The demo pilot's one public flight, by its pilot: not by its
        // place in the list, which grows with the development mode.
        let rows = app.descendants(matching: .any).matching(identifier: "public-flight-row")
        let demo = rows.matching(NSPredicate(format: "label CONTAINS %@", "Demo Pilot")).firstMatch
        XCTAssertTrue(demo.waitForExistence(timeout: 40))
        demo.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 40))

        // From the flight's details: lay another flight beside it.
        app.buttons["Flight details"].tap()
        let compare = element("flight-compare", in: app)
        XCTAssertTrue(compare.waitForExistence(timeout: 10))
        compare.tap()
        // What other pilots flew there that day, and the pilot's own.
        let other = element("compare-public-row", in: app)
        XCTAssertTrue(other.waitForExistence(timeout: 40))
        XCTAssertTrue(labelled(containing: "Your logbook holds no other flight", in: app).exists)
        sleep(1)
        keepScreenshot("compare-1-choose")
        other.tap()

        // Both flights on the page: the second track on the map, its
        // altitude in the profile, and a line that says whose it is.
        let beside = element("flight-beside", in: app)
        XCTAssertTrue(beside.waitForExistence(timeout: 40))
        sleep(3)
        keepScreenshot("compare-2-beside")

        // The two in numbers.
        beside.tap()
        XCTAssertTrue(app.navigationBars["Side by side"].waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "Airtime", in: app).waitForExistence(timeout: 10))
        XCTAssertTrue(labelled(containing: "Highest", in: app).exists)
        sleep(1)
        keepScreenshot("compare-3-numbers")

        // Taken away again, the page is the one flight's as before. The
        // button is under the numbers, below what a half-high sheet shows.
        let end = element("compare-end", in: app)
        for _ in 0..<4 where !(end.exists && end.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(end.waitForExistence(timeout: 10))
        end.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 10))
        XCTAssertFalse(element("flight-beside", in: app).exists)
    }

    /// The ranking of a month, a pilot found by name, and the pilot's
    /// page with what their public flights add up to. Runs against a
    /// Venti server in development mode, whose made-up pilot Lena Probe
    /// shares three flights with everyone, of May, June and July 2026.
    func testAPilotIsFoundRankedByMonthAndAddedUp() throws {
        try findAPilotAndTheRankingOfAMonth(dark: false, prefix: "pilots")
    }

    /// The same three screens in the dark.
    func testAPilotIsFoundRankedByMonthAndAddedUpInTheDark() throws {
        try findAPilotAndTheRankingOfAMonth(dark: true, prefix: "pilots-dark")
    }

    private func findAPilotAndTheRankingOfAMonth(dark: Bool, prefix: String) throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, dark: dark, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        openTab("League", in: app)

        // The ranking of 2026, and under the years its months that have
        // one, after the year itself. May is far right in the row.
        let ranking = app.buttons["Ranking"]
        XCTAssertTrue(ranking.waitForExistence(timeout: 15))
        ranking.tap()
        let year = app.buttons["2026"].firstMatch
        XCTAssertTrue(year.waitForExistence(timeout: 30))
        year.tap()
        let may = app.buttons["May"].firstMatch
        XCTAssertTrue(may.waitForExistence(timeout: 30))
        for _ in 0..<4 where !isOnScreen(may, in: app) {
            ["September", "August", "July", "June"]
                .map { app.buttons[$0].firstMatch }
                .first { isOnScreen($0, in: app) }?
                .swipeLeft()
            usleep(700_000)
        }
        may.tap()
        // The page says which month, and Lena's flight of May is the one
        // that scores in it.
        XCTAssertTrue(labelled(containing: "May 2026", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(labelled(containing: "Place 1, Lena Probe", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("\(prefix)-1-ranking-of-a-month")

        // A pilot is found by two letters of the name, in the field over
        // everyone's flights.
        let everyone = app.buttons["Everyone"]
        everyone.tap()
        XCTAssertTrue(element("public-flight-row", in: app).waitForExistence(timeout: 30))
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 15))
        field.tap()
        field.typeText("le")
        let found = labelled(containing: "Lena Probe (@lena), 3 public flights", in: app)
        XCTAssertTrue(found.waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("\(prefix)-2-pilot-found")

        // The line opens the pilot's page: the totals over the flights.
        found.tap()
        let totals = app.descendants(matching: .any).matching(identifier: "pilot-total")
        let points = totals.matching(NSPredicate(format: "label CONTAINS %@", "Points, best 3 flights")).firstMatch
        XCTAssertTrue(points.waitForExistence(timeout: 30))
        XCTAssertEqual(points.value as? String, "28.19")
        let site = labelled(containing: "Musterkogel West, 2 flights", in: app)
        XCTAssertTrue(site.waitForExistence(timeout: 15))
        XCTAssertTrue(element("flown-months", in: app).exists)
        sleep(1)
        keepScreenshot("\(prefix)-3-pilot-totals")

        // A take-off flown from opens the site's page. This step only
        // adds a screenshot.
        if isOnScreen(site, in: app) {
            site.tap()
            if element("site-tier", in: app).waitForExistence(timeout: 20) {
                sleep(1)
                keepScreenshot("\(prefix)-4-pilot-site")
            }
        }
    }

    /// A region's map is downloaded from the pilot's server and kept, and
    /// the cockpit then draws its map from it. Runs against a Venti server
    /// whose development mode hands out the region that comes with the
    /// app; without one, or with one that hands out no maps, it is left
    /// out.
    func testAMapIsDownloadedForFlyingWithoutANetwork() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        openTab("Fly", in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 30))

        // Before take-off the Fly tab says whether there is a map for
        // flying without a network, and leads to the regions.
        let before = element("map-before-takeoff", in: app)
        XCTAssertTrue(before.waitForExistence(timeout: 30))
        before.tap()
        // On a runner whose simulator has only just started, the app can
        // be slow to answer for half a minute, and a tap made then is
        // sometimes lost: the page is asked for once more.
        if !app.navigationBars["Offline maps"].waitForExistence(timeout: 20) {
            before.tap()
        }
        XCTAssertTrue(app.navigationBars["Offline maps"].waitForExistence(timeout: 30))
        let region = element("map-region", in: app)
        if !region.waitForExistence(timeout: 30) {
            if element("maps-none", in: app).exists {
                throw XCTSkip("The server hands out no maps: it was started without --maps.")
            }
            return XCTFail("The server's maps did not appear")
        }
        XCTAssertTrue(labelled(containing: "Tegernsee valley", in: app).exists)
        sleep(1)
        keepScreenshot("maps-1-offered")

        // The pilot is told the size before anything is fetched.
        element("map-download", in: app).tap()
        let go = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Download 7")).firstMatch
        XCTAssertTrue(go.waitForExistence(timeout: 10))
        go.tap()
        // Seven megabytes from the same machine: fetched, checked against
        // what the server said, and kept.
        XCTAssertTrue(element("map-delete", in: app).waitForExistence(timeout: 90))
        XCTAssertTrue(labelled(containing: "On this device", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("maps-2-kept")

        // Back on the Fly tab, the region is named among what to check.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.buttons["Open the cockpit"].waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Tegernsee valley", in: app).waitForExistence(timeout: 10))

        // The sample flight is over the Tegernsee, inside the region: the
        // cockpit draws its map from the download and says so.
        choose("20 times faster", from: app.buttons["Replay the sample flight"], in: app)
        XCTAssertTrue(element("Altitude", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(element("map-kept", in: app).waitForExistence(timeout: 30))
        // Time for the map to be made anew and to draw.
        sleep(6)
        keepScreenshot("maps-3-cockpit")
        // The height over the ground is read from the region's elevation.
        XCTAssertTrue(saying("over ground", in: app).waitForExistence(timeout: 30))

        // The wind on the ground, from the server's wind stations: the
        // nearest in a badge, and all of them in the wind's sheet, which
        // begins with them when it is opened from that badge. The
        // development mode's stations stand fifty kilometres off and
        // more, and the badge says so.
        let ground = element("ground-wind-badge", in: app)
        XCTAssertTrue(ground.waitForExistence(timeout: 120))
        tapWhenStill(ground)
        XCTAssertTrue(element("ground-wind-row", in: app).waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("maps-4-wind-on-the-ground")
        app.buttons["Done"].tap()
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

        // The aerial view: photographs from above under the flight, at
        // the round layers button. It is one choice for every map, so the
        // map tab shows it too, until the button there switches it back.
        let aerial = app.buttons["map-aerial"]
        XCTAssertTrue(aerial.waitForExistence(timeout: 15))
        // The choice is kept between launches: an earlier run that
        // stopped half-way may have left it on.
        if aerial.isSelected {
            aerial.tap()
        }
        XCTAssertFalse(aerial.isSelected)
        aerial.tap()
        XCTAssertTrue(aerial.isSelected)
        // Whose photographs they are is said on the map while they show.
        XCTAssertTrue(element("photo-credits", in: app).waitForExistence(timeout: 10))
        sleep(12)
        keepScreenshot("online-3-flight-aerial")

        // Back to the logbook: a flight's page has no tabs.
        app.navigationBars.buttons.firstMatch.tap()
        openTab("Map", in: app)
        // The map tab names them beside its credits button, as every map
        // does: the makers of the photographs in view. This test names no
        // Venti server, so the style has no Austrian layer and basemap.at
        // is not named.
        let photos = element("photo-credits", in: app)
        XCTAssertTrue(photos.waitForExistence(timeout: 30))
        sleep(12)
        XCTAssertTrue(
            photos.label.contains("Photos: ") && photos.label.contains("Bayerische Vermessungsverwaltung · EOX"),
            photos.label
        )
        keepScreenshot("online-4-map-aerial")
        // And in full behind the credits button, with the map's own.
        app.buttons["map-credits"].tap()
        let credits = element("wind-credits", in: app)
        XCTAssertTrue(credits.waitForExistence(timeout: 10))
        XCTAssertTrue(credits.label.contains("Photos: "), credits.label)
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)
        // The aerial view is a switch in the layers sheet here.
        XCTAssertTrue(openLayers(in: app))
        XCTAssertTrue(isOn(app.switches["map-aerial"]))
        setLayer("map-aerial", on: false, in: app)
    }

    /// The filter of the flying sites: take-offs for a wind from the
    /// south, around the Tegernsee, where the development server has
    /// take-offs that name their wind and take-offs that name none. The
    /// filter says what it is and what it leaves, the map draws the ones
    /// that fit and, muted, the ones that say nothing, the list has the
    /// two apart, and the filter is kept for the next launch. Then a
    /// landing field and a take-off that their source maps as areas,
    /// close up. Skipped without a server.
    func testTheSitesFilterShowsWhatFitsAndWhatSaysNothing() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        // Preferences outlive a test: whatever becomes of this one, the
        // tests after it find a filter that asks nothing.
        addTeardownBlock { @MainActor in
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--all-sites", "--server", Self.nowhere]
            app.launch()
            app.terminate()
        }
        let valley = ["--server", server, "--map-at", "47.67,11.68,9.3"]
        var app = launch(bundledMap: true, extra: valley + ["--all-sites"])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("layer-sites", on: true, in: app)

        // The filter, from its button under the one that lists the sites.
        XCTAssertTrue(waitForCredit("Sites", in: app))
        XCTAssertFalse(element("sites-filter-line", in: app).exists)
        let filter = layersRow("sites-filter", in: app)
        XCTAssertTrue(filter.exists)
        filter.tap()
        let uses = element("sites-shown", in: app)
        XCTAssertTrue(uses.waitForExistence(timeout: 15))
        uses.buttons["Take-offs"].tap()
        // The sixteen points scroll: "S" is brought into the row.
        let winds = element("sites-wind", in: app)
        XCTAssertTrue(winds.waitForExistence(timeout: 10))
        let south = winds.buttons["S"]
        for _ in 0..<6 where !(south.exists && winds.frame.contains(south.frame)) {
            winds.coordinate(withNormalizedOffset: CGVector(dx: 0.8, dy: 0.5))
                .press(forDuration: 0.05, thenDragTo: winds.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)))
            usleep(600_000)
        }
        south.tap()
        XCTAssertTrue(south.isSelected)
        // At half height, over the map it changes.
        sleep(2)
        keepScreenshot("sites-filter-1-control")

        // The sheet opens at half height: pulled up, it shows what the
        // filter is, what it leaves, what says nothing, and the note.
        let leaves = element("sites-filter-words", in: app)
        let window = app.windows.firstMatch
        for _ in 0..<3 where !isOnScreen(leaves, in: app) {
            window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.85))
                .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35)))
            usleep(800_000)
        }
        XCTAssertTrue(leaves.waitForExistence(timeout: 10))
        XCTAssertEqual(element("sites-filter-on", in: app).label, "Take-offs for a wind from S")
        let fits = leaves.label
        XCTAssertNotNil(
            fits.range(of: #"^[1-9][0-9]* of [0-9]+ take-offs? fits? a wind from S$"#, options: .regularExpression),
            "What the filter leaves reads: \(fits)"
        )
        let unsaid = element("sites-filter-unsaid", in: app).label
        XCTAssertNotNil(
            unsaid.range(of: #"^[1-9][0-9]* take-offs? gives? no wind directions$"#, options: .regularExpression),
            "What says nothing reads: \(unsaid)"
        )
        sleep(1)
        keepScreenshot("sites-filter-1b-what-it-leaves")
        app.buttons["Done"].tap()

        // The map: the line of the filter with the same words, the
        // take-offs that fit as ever, the ones that say nothing muted.
        let line = element("sites-filter-line", in: app)
        XCTAssertTrue(line.waitForExistence(timeout: 15))
        XCTAssertTrue(line.label.contains("Take-offs for a wind from S"))
        XCTAssertTrue(line.label.contains(fits))
        XCTAssertTrue(line.label.contains(unsaid))
        sleep(3)
        keepScreenshot("sites-filter-2-map")
        // The row in the layers sheet says that a filter is on, and which.
        XCTAssertEqual(layersRow("sites-filter", in: app).value as? String, "Take-offs for a wind from S")

        // The list: the ones that fit under what the filter leaves, then
        // the ones that say nothing under their heading and its sentence.
        layersRow("Flying sites", in: app).tap()
        XCTAssertTrue(app.navigationBars["Flying sites"].waitForExistence(timeout: 15))
        XCTAssertTrue(element("site-row", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(saying(fits, in: app).exists)
        XCTAssertTrue(saying(unsaid, in: app).exists)
        XCTAssertTrue(element("site-row-unsaid", in: app).exists)
        XCTAssertTrue(saying("They are shown apart, not left out.", in: app).exists)
        sleep(1)
        keepScreenshot("sites-filter-3-list")
        app.buttons["Done"].tap()

        // Kept as the pilot left it: the next launch, in the dark, has
        // the same filter on.
        app.terminate()
        app = launch(bundledMap: true, dark: true, extra: valley)
        let kept = element("sites-filter-line", in: app)
        XCTAssertTrue(kept.waitForExistence(timeout: 90))
        XCTAssertTrue(kept.label.contains("Take-offs for a wind from S"))
        waitForCredit("Sites", in: app)
        sleep(4)
        keepScreenshot("sites-filter-4-map-dark")
        app.terminate()

        // A landing field that its source maps as an area, close up: its
        // outline under its sign. And a take-off's.
        for (name, place) in [
            ("sites-filter-5-landing-field", "47.6924,11.8865,15"),
            ("sites-filter-6-takeoff-field", "47.6791,11.9051,16.5"),
        ] {
            app = launch(bundledMap: true, extra: ["--server", server, "--all-sites", "--map-at", place])
            XCTAssertTrue(waitForCredit("Sites", in: app, timeout: 90))
            XCTAssertFalse(element("sites-filter-line", in: app).exists)
            sleep(4)
            keepScreenshot(name)
            app.terminate()
        }
    }
}

/// Adding and correcting a flying site, against the development server
/// with an account the test makes.
extension VentiUITests {
    /// Makes an account on the development server, over its own API, and
    /// returns what it signs in with. The password is made up here, for
    /// this run's server, which is stopped when the run ends.
    private func makeAccount(on server: String) async throws -> (handle: String, password: String) {
        let handle = "sites\(Int.random(in: 100_000...999_999))"
        let password = "made-up-\(UUID().uuidString.lowercased())"
        var request = URLRequest(url: try XCTUnwrap(URL(string: server + "/v1/accounts")))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(
            withJSONObject: ["handle": handle, "display_name": "Site Tester", "password": password]
        )
        let (_, response) = try await URLSession.shared.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        XCTAssertTrue((200..<300).contains(status), "The development server made no account: \(status)")
        return (handle, password)
    }

    /// The pilot's alarms, signed in as the development server's demo
    /// account: "Alerts" under the account lists its two alarms and, under
    /// them, what fired (the Patscherkofel's line); "Alert me" on a wind
    /// station's page opens the form, whose first try to save shows the
    /// core's fault at the directions; with a direction chosen it saves,
    /// and the alarm is in the list. Then the list and a form in the dark.
    func testAPilotSetsAnAlarmAndSeesWhatFired() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        // The map opens on the Zugspitze, whose station the wind's list
        // then has.
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.42,10.98,10"])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        openAlerts(in: app)
        XCTAssertTrue(labelled(containing: "Patscherkofel north", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Nordkette cross-country", in: app).exists)
        let fired = element("alerts-fired-line", in: app)
        XCTAssertTrue(fired.waitForExistence(timeout: 30))
        XCTAssertTrue(fired.label.contains("Patscherkofel"), fired.label)
        sleep(1)
        keepScreenshot("alerts-1-list")

        // "Alert me" on the Zugspitze's page.
        openTab("Map", in: app)
        XCTAssertTrue(waitForCredit("Wind", in: app))
        let stations = layersRow("Wind stations", in: app, timeout: 30)
        XCTAssertTrue(stations.exists)
        stations.tap()
        XCTAssertTrue(app.navigationBars["Wind stations"].waitForExistence(timeout: 15))
        let summit = labelled(containing: "Zugspitze", in: app)
        XCTAssertTrue(summit.waitForExistence(timeout: 30))
        summit.tap()
        XCTAssertTrue(element("wind-chart", in: app).waitForExistence(timeout: 30))
        bring("alert-me", into: app).tap()
        let line = element("alert-line", in: app)
        XCTAssertTrue(line.waitForExistence(timeout: 15))
        XCTAssertTrue(line.label.hasPrefix("Wind at Zugspitze"), line.label)
        sleep(1)
        keepScreenshot("alerts-2-form")

        // Neither directions nor speeds: the core's fault, at its field.
        element("alert-save", in: app).tap()
        let fault = element("alert-fault-rule.directions", in: app)
        XCTAssertTrue(fault.waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("alerts-3-fault")
        let north = element("alert-from", in: app).buttons["N"].firstMatch
        XCTAssertTrue(north.waitForExistence(timeout: 10))
        north.tap()
        XCTAssertTrue(line.label.contains("from N"), line.label)
        for _ in 0..<10 where fault.exists {
            usleep(300_000)
        }
        XCTAssertFalse(fault.exists)
        element("alert-save", in: app).tap()
        // The first alarm asks whether Venti may notify.
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow"]
        if allow.waitForExistence(timeout: 8) {
            allow.tap()
        }
        XCTAssertTrue(element("alert-me", in: app).waitForExistence(timeout: 20))
        XCTAssertFalse(line.exists)
        // The station's page is pushed in the stations' sheet over the
        // map: back to the list, and the list closed with "Done".
        for _ in 0..<4 where element("wind-chart", in: app).exists || app.navigationBars["Wind stations"].exists {
            let done = app.navigationBars["Wind stations"].buttons["Done"]
            if done.exists {
                done.tap()
            } else {
                app.navigationBars.buttons.element(boundBy: 0).tap()
            }
            sleep(1)
        }

        openAlerts(in: app)
        XCTAssertTrue(labelled(containing: "Wind at Zugspitze from N", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Patscherkofel north", in: app).exists)
        sleep(1)
        keepScreenshot("alerts-4-list-with-the-new-one")

        // In the dark: the list, and the Patscherkofel's alarm in its form.
        app.terminate()
        let dark = launch(bundledMap: true, dark: true, extra: ["--server", server])
        XCTAssertTrue(dark.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: dark)
        openAlerts(in: dark)
        // The alarms come the oldest first: the Patscherkofel's is the first.
        let row = element("alert-row", in: dark)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        XCTAssertTrue(row.label.contains("Patscherkofel north"), row.label)
        XCTAssertTrue(element("alerts-fired-line", in: dark).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("alerts-5-list-dark")
        row.tap()
        let form = element("alert-line", in: dark)
        XCTAssertTrue(form.waitForExistence(timeout: 15))
        XCTAssertTrue(form.label.hasPrefix("Wind at Patscherkofel from N to NE"), form.label)
        sleep(1)
        keepScreenshot("alerts-6-form-dark")
    }

    /// The handle and password form of the account screen: behind "Sign
    /// in with a handle and password" where Venti offers Google, at once
    /// where it does not. Returns the handle's field.
    private func openThePasswordForm(in app: XCUIApplication) -> XCUIElement {
        let handle = app.textFields["Handle"]
        let withPassword = element("account-with-password", in: app)
        for _ in 0..<30 where !handle.exists {
            if withPassword.exists {
                withPassword.tap()
                break
            }
            usleep(500_000)
        }
        return handle
    }

    /// "Continue with Google" through the development server's made-up
    /// Google (the launch argument `--google-dev`): a new pilot passes the
    /// handle step with the suggested handle and lands signed in, showing
    /// the Google email; signed out and in again with the same email, the
    /// pilot lands signed in without the handle step. The screens light,
    /// and the signed-in account once more in the dark.
    func testAPilotContinuesWithGoogle() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let email = "ui.test@example.com"
        let google = ["--server", server, "--google-dev", email, "UI Test"]
        let app = launch(bundledMap: true, extra: google)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        openAccount(in: app)

        // Signed out: Google first, the password form behind its button.
        let continueWithGoogle = element("account-google", in: app)
        XCTAssertTrue(continueWithGoogle.waitForExistence(timeout: 30))
        XCTAssertEqual(continueWithGoogle.label, "Continue with Google")
        XCTAssertTrue(element("account-with-password", in: app).exists)
        XCTAssertFalse(app.textFields["Handle"].exists)
        sleep(1)
        keepScreenshot("google-1-signed-out")

        // A new pilot: the handle step, the suggestion filled in.
        continueWithGoogle.tap()
        let handle = element("google-handle", in: app)
        XCTAssertTrue(handle.waitForExistence(timeout: 30))
        XCTAssertEqual(handle.value as? String, "ui_test")
        XCTAssertEqual(element("google-display-name", in: app).value as? String, "UI Test")
        XCTAssertTrue(labelled(containing: "@ui_test is free.", in: app).exists)
        XCTAssertTrue(element("google-handle-title", in: app).exists)
        sleep(1)
        keepScreenshot("google-2-handle-step")
        element("google-create", in: app).tap()

        // Signed in, with the Google email shown to the pilot.
        XCTAssertTrue(labelled(containing: "@ui_test", in: app).waitForExistence(timeout: 30))
        let shown = element("account-google-email", in: app)
        XCTAssertTrue(shown.waitForExistence(timeout: 15))
        XCTAssertTrue(shown.label.contains(email), shown.label)
        XCTAssertTrue(element("account-unlink-google", in: app).exists)
        XCTAssertTrue(app.buttons["Sync now"].waitForExistence(timeout: 60))
        sleep(1)
        keepScreenshot("google-3-signed-in")

        // Signed out, and in again: no handle step.
        bring("account-sign-out", into: app).tap()
        let confirm = element("account-sign-out-confirm", in: app)
        XCTAssertTrue(confirm.waitForExistence(timeout: 10))
        confirm.tap()
        XCTAssertTrue(continueWithGoogle.waitForExistence(timeout: 30))
        continueWithGoogle.tap()
        XCTAssertTrue(element("account-google-email", in: app).waitForExistence(timeout: 30))
        XCTAssertFalse(handle.exists)
        XCTAssertTrue(labelled(containing: "@ui_test", in: app).exists)

        // In the dark, signed in by Google on a second launch.
        app.terminate()
        let dark = launch(bundledMap: true, dark: true, extra: google)
        XCTAssertTrue(dark.buttons["Show my position"].waitForExistence(timeout: 90))
        openAccount(in: dark)
        let again = element("account-google", in: dark)
        XCTAssertTrue(again.waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("google-4-signed-out-dark")
        again.tap()
        XCTAssertTrue(element("account-google-email", in: dark).waitForExistence(timeout: 30))
        XCTAssertTrue(dark.buttons["Sync now"].waitForExistence(timeout: 60))
        sleep(1)
        keepScreenshot("google-5-signed-in-dark")
    }

    /// Opens Settings, Account.
    private func openAccount(in app: XCUIApplication) {
        openTab("Settings", in: app)
        let link = element("account-link", in: app)
        for _ in 0..<4 where !(link.exists && link.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        link.tap()
    }

    /// Opens "Alerts" under Settings, Account.
    private func openAlerts(in app: XCUIApplication) {
        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        bring("alerts-link", into: app).tap()
        XCTAssertTrue(app.navigationBars["Alerts"].waitForExistence(timeout: 15))
    }

    /// Signs in under Settings, Account, and comes back to the map.
    private func signIn(_ account: (handle: String, password: String), in app: XCUIApplication) {
        openTab("Settings", in: app)
        let link = element("account-link", in: app)
        for _ in 0..<4 where !(link.exists && link.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        link.tap()
        let handle = openThePasswordForm(in: app)
        XCTAssertTrue(handle.waitForExistence(timeout: 15))
        handle.tap()
        handle.typeText(account.handle)
        let password = app.secureTextFields["Password"]
        password.tap()
        password.typeText(account.password)
        element("account-submit", in: app).tap()
        XCTAssertTrue(labelled(containing: "@\(account.handle)", in: app).waitForExistence(timeout: 30))
        // The iPhone offers to keep the password. Not in a test.
        for asking in [app, XCUIApplication(bundleIdentifier: "com.apple.springboard")] {
            let notNow = asking.buttons["Not Now"]
            if notNow.waitForExistence(timeout: 3) {
                notNow.tap()
                break
            }
        }
        openTab("Map", in: app)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 30))
        sleep(3)
    }

    /// A long press on the map, so far from its middle, and "Add a site
    /// here" in what it opens.
    private func pressTheMap(dx: CGFloat, dy: CGFloat, in app: XCUIApplication) -> XCUIElement {
        element("overview-map", in: app)
            .coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .withOffset(CGVector(dx: dx, dy: dy))
            .press(forDuration: 1.2)
        let entry = element("add-site-here", in: app)
        XCTAssertTrue(entry.waitForExistence(timeout: 20))
        return entry
    }

    /// Brings an element of a list into the screen by swiping the list.
    private func bring(_ identifier: String, into app: XCUIApplication, down: Bool = false) -> XCUIElement {
        let wanted = element(identifier, in: app)
        for _ in 0..<8 where !isOnScreen(wanted, in: app) {
            if down {
                app.swipeDown()
            } else {
                app.swipeUp()
            }
            usleep(500_000)
        }
        XCTAssertTrue(wanted.waitForExistence(timeout: 10), identifier)
        return wanted
    }

    /// The first of the form's choices of one kind: "Take-off" among the uses.
    private func firstChoice(_ kind: String, in app: XCUIApplication) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH %@", kind + "-")).firstMatch
    }

    /// Preferences outlive a test: whatever becomes of a test that
    /// switches the places from pilots on, the tests after it find them
    /// off and a filter that asks nothing.
    private func leaveTheSitesFilterAskingNothing() {
        addTeardownBlock { @MainActor in
            let app = XCUIApplication()
            app.launchArguments = ["--ui-testing", "--all-sites", "--server", Self.nowhere]
            app.launch()
            app.terminate()
        }
    }

    /// Brings an element of a sheet that opens at half height into the
    /// screen: the first drag pulls the sheet up, the next ones scroll it.
    private func bringInSheet(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        let wanted = element(identifier, in: app)
        let window = app.windows.firstMatch
        for _ in 0..<5 where !isOnScreen(wanted, in: app) {
            window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.85))
                .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35)))
            usleep(800_000)
        }
        XCTAssertTrue(wanted.waitForExistence(timeout: 10), identifier)
        return wanted
    }

    /// Opens the filter of the sites from its button on the map, and
    /// brings the switch "Places from pilots" at its foot into view.
    private func openThePilotsSwitch(in app: XCUIApplication) -> XCUIElement {
        let filter = layersRow("sites-filter", in: app)
        XCTAssertTrue(filter.exists)
        filter.tap()
        XCTAssertTrue(element("sites-shown", in: app).waitForExistence(timeout: 15))
        let layer = bringInSheet("sites-from-pilots", in: app)
        _ = bringInSheet("sites-from-pilots-about", in: app)
        return layer
    }

    /// Whether a switch is on.
    private func isOn(_ toggle: XCUIElement) -> Bool {
        (toggle.value as? String) == "1"
    }

    /// Sets a switch of a list. A tap on the row's middle does not always
    /// reach the switch itself: its right end does.
    private func set(_ toggle: XCUIElement, on: Bool) {
        if isOn(toggle) != on {
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
            usleep(800_000)
        }
        if isOn(toggle) != on {
            toggle.tap()
            usleep(800_000)
        }
        XCTAssertEqual(isOn(toggle), on)
    }

    /// A pilot adds a place pilots take off from beside the made-up
    /// "Musterkogel West", which goes to the crowdsourced tier: it is on
    /// the map and in its list only once the places from pilots are
    /// switched on. The pilot then corrects its height, reads the two
    /// revisions, undoes the correction and removes the site; and is
    /// refused a site on top of "Musterkogel West", with the offer to open
    /// that one.
    func testAPilotAddsCorrectsAndRemovesASite() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let account = try await makeAccount(on: server)
        // "Musterkogel West" in the map's middle, so close that a finger's
        // width is a few metres: at zoom 19 a point is a tenth of a metre.
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.3,10.9,19", "--all-sites"])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)

        // Some 27 m north-west of it: a place of its own. An account the
        // test made is an ordinary pilot: the entry and the form are the
        // crowdsourced tier's, in the core's words.
        let entry = pressTheMap(dx: -140, dy: -230, in: app)
        XCTAssertEqual(entry.label, "Add a place pilots take off from")
        sleep(2)
        keepScreenshot("site-edit-1-entry-on-the-map")
        entry.tap()

        let name = element("site-name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 15))
        XCTAssertTrue(app.navigationBars["A place pilots take off from"].exists)
        sleep(1)
        keepScreenshot("site-edit-2-form-empty")

        // Sent empty: the core's faults at their fields, and nothing sent.
        bring("site-send", into: app).tap()
        XCTAssertTrue(element("site-fault", in: app).waitForExistence(timeout: 10))
        sleep(1)
        keepScreenshot("site-edit-3-form-with-a-fault")

        name.tap()
        name.typeText("Testkogel\n")
        firstChoice("site-use", in: app).tap()
        let height = bring("site-height", into: app)
        height.tap()
        height.typeText("1500\n")
        let wind = firstChoice("site-wind", in: app)
        if wind.exists, isOnScreen(wind, in: app) {
            wind.tap()
        }
        let send = bring("site-send", into: app)
        XCTAssertTrue(element("site-giving", in: app).exists)
        XCTAssertTrue(element("site-licence", in: app).exists)
        XCTAssertEqual(send.label, "Add as a place from pilots")
        keepScreenshot("site-edit-4-giving-and-send")
        send.tap()

        // The core's line, on the new site's page, which says its tier.
        let said = element("site-said", in: app)
        XCTAssertTrue(said.waitForExistence(timeout: 30))
        XCTAssertTrue(said.label.contains("Added"), said.label)
        XCTAssertEqual(element("site-tier", in: app).label, "From pilots")
        // Nobody has said yet that they took off here: the core's line.
        let nobody = element("site-confirmations-none", in: app)
        XCTAssertTrue(nobody.waitForExistence(timeout: 20))
        XCTAssertTrue(nobody.label.hasPrefix("No pilot has said yet"), nobody.label)
        sleep(2)
        keepScreenshot("site-edit-5-new-site-page")
        app.buttons["Done"].firstMatch.tap()
        sleep(3)

        // The places from pilots are off: the new place is not in the
        // list of what the map shows, and the partner's take-off is.
        layersRow("Flying sites", in: app).tap()
        XCTAssertTrue(labelled(containing: "Musterkogel West", in: app).waitForExistence(timeout: 20))
        XCTAssertFalse(labelled(containing: "Testkogel", in: app).exists)
        app.buttons["Done"].firstMatch.tap()
        sleep(1)

        // Switched on, at the foot of the filter: it is on the map.
        set(openThePilotsSwitch(in: app), on: true)
        app.buttons["Done"].firstMatch.tap()
        sleep(3)
        keepScreenshot("site-edit-6-on-the-map")

        // And in the list of what the map shows, marked.
        layersRow("Flying sites", in: app).tap()
        let row = labelled(containing: "Testkogel", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 20))
        XCTAssertTrue(row.label.contains("From pilots"), row.label)
        row.tap()

        // Correct its height.
        bring("site-correct", into: app).tap()
        let corrected = bring("site-height", into: app)
        XCTAssertTrue(corrected.waitForExistence(timeout: 15))
        corrected.tap()
        corrected.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 6) + "1520\n")
        let why = bring("site-comment", into: app)
        why.tap()
        why.typeText("measured again\n")
        bring("site-send", into: app).tap()
        let saved = bring("site-said", into: app, down: true)
        XCTAssertTrue(saved.label.contains("revision 2"), saved.label)
        keepScreenshot("site-edit-7-corrected")

        // Two revisions, the change in words; then undone.
        bring("site-history", into: app).tap()
        let change = element("site-revision-change", in: app)
        XCTAssertTrue(change.waitForExistence(timeout: 20))
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "site-revision").count, 2)
        XCTAssertTrue(labelled(containing: "1500 m → 1520 m", in: app).exists)
        XCTAssertTrue(labelled(containing: "measured again", in: app).exists)
        sleep(1)
        keepScreenshot("site-edit-8-history")
        element("site-undo", in: app).tap()
        let putBack = app.alerts.buttons["Put back"]
        XCTAssertTrue(putBack.waitForExistence(timeout: 10))
        putBack.tap()
        XCTAssertTrue(element("site-history-said", in: app).waitForExistence(timeout: 30))
        sleep(1)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "site-revision").count, 3)
        keepScreenshot("site-edit-9-undone")
        app.navigationBars["History"].buttons.firstMatch.tap()

        // Remove it.
        bring("site-remove", into: app).tap()
        let remove = app.alerts.buttons["Remove this site"]
        XCTAssertTrue(remove.waitForExistence(timeout: 10))
        remove.tap()
        sleep(2)
        let removed = bring("site-said", into: app, down: true)
        XCTAssertTrue(removed.label.contains("Removed"), removed.label)
        keepScreenshot("site-edit-10-removed")
        // What the page offers now is the core's answer for a removed
        // site: its history, and the line that it is on no map.
        let gone = bring("site-offers-note", into: app)
        XCTAssertTrue(gone.label.hasPrefix("This site was removed"), gone.label)
        XCTAssertFalse(element("site-correct", in: app).exists)
        XCTAssertFalse(element("site-remove", in: app).exists)
        XCTAssertFalse(element("site-confirm", in: app).exists)
        keepScreenshot("site-edit-10b-removed-offers")
        element("site-history", in: app).tap()
        let bringBack = element("site-bring-back", in: app)
        XCTAssertTrue(bringBack.waitForExistence(timeout: 20))
        XCTAssertEqual(bringBack.label, "Bring this site back")
        XCTAssertFalse(element("site-undo", in: app).exists)
        sleep(1)
        keepScreenshot("site-edit-10c-removed-history")
        app.navigationBars["History"].buttons.firstMatch.tap()
        sleep(1)
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Done"].firstMatch.tap()
        sleep(2)

        // On top of "Musterkogel West": some ten metres south of it, under
        // its sign. The server refuses, and the core names the site.
        pressTheMap(dx: 0, dy: 100, in: app).tap()
        let second = element("site-name", in: app)
        XCTAssertTrue(second.waitForExistence(timeout: 15))
        second.tap()
        second.typeText("Too close\n")
        firstChoice("site-use", in: app).tap()
        bring("site-send", into: app).tap()
        let inTheWay = element("site-in-the-way", in: app)
        XCTAssertTrue(inTheWay.waitForExistence(timeout: 30))
        XCTAssertEqual(inTheWay.label, "Open Musterkogel West")
        XCTAssertTrue(element("site-problem", in: app).label.contains("Musterkogel West"))
        _ = bring("site-in-the-way", into: app)
        keepScreenshot("site-edit-11-too-close")
        inTheWay.tap()
        XCTAssertTrue(app.staticTexts["Musterkogel West"].waitForExistence(timeout: 20))
        sleep(2)
        keepScreenshot("site-edit-12-the-site-in-the-way")
    }

    /// The form in the dark.
    func testTheSiteFormInTheDark() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let account = try await makeAccount(on: server)
        let app = launch(
            bundledMap: true, dark: true, extra: ["--server", server, "--map-at", "47.3,10.9,19", "--all-sites"]
        )
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)
        pressTheMap(dx: -140, dy: -230, in: app).tap()
        let name = element("site-name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 15))
        name.tap()
        name.typeText("Testkogel\n")
        firstChoice("site-use", in: app).tap()
        sleep(1)
        keepScreenshot("site-edit-form-dark-1")
        _ = bring("site-send", into: app)
        keepScreenshot("site-edit-form-dark-2-giving")
    }
}

/// The two tiers of Venti's own sites, against the development server:
/// its partner's take-off "Musterkogel West" and landing "Musterwiese",
/// and the crowdsourced take-off "Beispieljoch".
extension VentiUITests {
    /// The three made-up sites in one view of the map.
    private static let lechtal = "47.29,10.9,12"

    /// With the places from pilots off, the crowdsourced take-off is not
    /// in the list of what the map shows; switched on at the foot of the
    /// filter, it is, marked. A pilot the test made says of it "I took
    /// off here and it was good" for yesterday with a line, reads the
    /// core's sentence and their line among the pilots', and takes it
    /// back. A partner's site offers that pilot no correcting, and the
    /// core's line says why.
    func testAPilotSwitchesThePlacesFromPilotsOnAndConfirmsOne() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let account = try await makeAccount(on: server)
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", Self.lechtal, "--all-sites"])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)
        setLayer("layer-sites", on: true, in: app)

        // Off: the partner's sites, and no place from pilots.
        layersRow("Flying sites", in: app).tap()
        XCTAssertTrue(labelled(containing: "Musterkogel West", in: app).waitForExistence(timeout: 30))
        XCTAssertFalse(labelled(containing: "Beispieljoch", in: app).exists)
        XCTAssertFalse(labelled(containing: "From pilots", in: app).exists)
        app.buttons["Done"].firstMatch.tap()
        sleep(1)

        // The switch at the foot of the filter, off and on.
        let layer = openThePilotsSwitch(in: app)
        XCTAssertEqual(layer.label, "Places from pilots")
        XCTAssertFalse(isOn(layer))
        sleep(1)
        keepScreenshot("site-tiers-1-filter-switch-off")
        set(layer, on: true)
        // The core's line under what the filter leaves.
        let among = element("sites-filter-from-pilots", in: app)
        for _ in 0..<3 where !isOnScreen(among, in: app) {
            app.swipeDown()
            usleep(600_000)
        }
        XCTAssertTrue(among.waitForExistence(timeout: 20))
        XCTAssertTrue(among.label.contains("from pilots"), among.label)
        _ = bringInSheet("sites-from-pilots-about", in: app)
        sleep(1)
        keepScreenshot("site-tiers-2-filter-switch-on")
        app.buttons["Done"].firstMatch.tap()

        // Counted as a filter being on: its line stands over the map.
        XCTAssertTrue(element("sites-filter-line", in: app).waitForExistence(timeout: 20))
        sleep(3)
        keepScreenshot("site-tiers-3-on-the-map")

        // In the list, marked.
        layersRow("Flying sites", in: app).tap()
        let row = labelled(containing: "Beispieljoch", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        XCTAssertTrue(row.label.contains("From pilots"), row.label)
        XCTAssertFalse(labelled(containing: "Musterkogel West", in: app).label.contains("From"))
        sleep(1)
        keepScreenshot("site-tiers-4-list")
        row.tap()

        // Its page: the tier by the name, one pilot's confirmation, the
        // sentence to say, and the notice.
        XCTAssertTrue(element("site-tier", in: app).waitForExistence(timeout: 20))
        XCTAssertEqual(element("site-tier", in: app).label, "From pilots")
        let count = element("site-confirmations", in: app)
        XCTAssertTrue(count.waitForExistence(timeout: 20))
        XCTAssertTrue(count.label.contains("1 pilot took off here"), count.label)
        let confirm = element("site-confirm", in: app)
        XCTAssertTrue(confirm.waitForExistence(timeout: 20))
        XCTAssertEqual(confirm.label, "I took off here and it was good")
        sleep(1)
        keepScreenshot("site-tiers-5-page-from-pilots")
        _ = bring("site-tier-note", into: app)
        keepScreenshot("site-tiers-6-page-notice-and-offers")

        // Said for yesterday, with a line.
        bring("site-confirm", into: app, down: true).tap()
        let day = element("site-confirm-day", in: app)
        XCTAssertTrue(day.waitForExistence(timeout: 15))
        day.tap()
        let yesterday = app.buttons["Yesterday"]
        XCTAssertTrue(yesterday.waitForExistence(timeout: 10))
        yesterday.tap()
        sleep(1)
        keepScreenshot("site-tiers-7-confirm-sheet")
        let note = element("site-confirm-note", in: app)
        note.tap()
        note.typeText("Smooth at noon")
        sleep(1)
        keepScreenshot("site-tiers-7b-confirm-sheet-with-a-line")
        bring("site-confirm-send", into: app).tap()

        // The core's sentence in the button's place, with the two ways on.
        let confirmed = element("site-confirmed", in: app)
        XCTAssertTrue(confirmed.waitForExistence(timeout: 30))
        XCTAssertTrue(confirmed.label.hasPrefix("You confirmed this site for"), confirmed.label)
        XCTAssertEqual(element("site-confirm", in: app).label, "Change the day")
        let withdraw = element("site-withdraw", in: app)
        XCTAssertEqual(withdraw.label, "Take my confirmation back")
        XCTAssertTrue(count.label.contains("2 pilots took off here"), count.label)
        sleep(1)
        keepScreenshot("site-tiers-8-page-after-confirming")

        // "Pilots who took off here", with the pilot's own line.
        count.tap()
        XCTAssertTrue(app.navigationBars["Pilots who took off here"].waitForExistence(timeout: 15))
        let mine = labelled(containing: "Smooth at noon", in: app)
        XCTAssertTrue(mine.waitForExistence(timeout: 20))
        XCTAssertTrue(mine.label.contains("@\(account.handle)"), mine.label)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "site-confirmation").count, 2)
        sleep(1)
        keepScreenshot("site-tiers-9-pilots-who-took-off-here")
        app.navigationBars["Pilots who took off here"].buttons.firstMatch.tap()

        // Taken back.
        bring("site-withdraw", into: app).tap()
        let back = app.alerts.buttons["Take my confirmation back"]
        XCTAssertTrue(back.waitForExistence(timeout: 10))
        XCTAssertTrue(app.alerts.staticTexts["Take your confirmation of this site back?"].exists)
        back.tap()
        XCTAssertTrue(
            labelled(containing: "Your confirmation of this site is taken back", in: app)
                .waitForExistence(timeout: 30)
        )
        let again = bring("site-confirm", into: app)
        XCTAssertEqual(again.label, "I took off here and it was good")
        XCTAssertFalse(element("site-confirmed", in: app).exists)
        keepScreenshot("site-tiers-10-taken-back")

        // A place from pilots is this pilot's to correct; a partner's is
        // not, and the core's line says why.
        XCTAssertTrue(bring("site-correct", into: app).exists)
        app.navigationBars.buttons.firstMatch.tap()
        let partners = labelled(containing: "Musterkogel West", in: app)
        XCTAssertTrue(partners.waitForExistence(timeout: 20))
        partners.tap()
        XCTAssertTrue(element("site-tier", in: app).waitForExistence(timeout: 20))
        XCTAssertEqual(element("site-tier", in: app).label, "From a partner")
        let why = bring("site-offers-note", into: app)
        XCTAssertTrue(why.label.hasPrefix("A partner of Venti added this site"), why.label)
        XCTAssertFalse(element("site-correct", in: app).exists)
        XCTAssertFalse(element("site-remove", in: app).exists)
        XCTAssertTrue(element("site-history", in: app).exists)
        XCTAssertTrue(element("site-report", in: app).exists)
        keepScreenshot("site-tiers-11-a-partners-site-offers-no-correcting")
    }

    /// A place from pilots' page in the dark, signed out: the core's line
    /// says what signing in would offer.
    func testAPlaceFromPilotsInTheDark() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let app = launch(
            bundledMap: true, dark: true, extra: ["--server", server, "--map-at", Self.lechtal, "--all-sites"]
        )
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("layer-sites", on: true, in: app)
        set(openThePilotsSwitch(in: app), on: true)
        sleep(1)
        keepScreenshot("site-tiers-dark-1-filter-switch-on")
        app.buttons["Done"].firstMatch.tap()
        layersRow("Flying sites", in: app).tap()
        let row = labelled(containing: "Beispieljoch", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        XCTAssertTrue(element("site-tier", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(element("site-confirmations", in: app).waitForExistence(timeout: 20))
        // Signed out: nothing to say or correct, and the line says why.
        XCTAssertFalse(element("site-confirm", in: app).exists)
        sleep(1)
        keepScreenshot("site-tiers-dark-2-page-from-pilots")
        let why = bring("site-offers-note", into: app)
        XCTAssertTrue(why.label.hasPrefix("Sign in to correct this site"), why.label)
        keepScreenshot("site-tiers-dark-3-page-offers")

        // Signed out, the core's entry for adding is its sentence about
        // signing in, and it opens the account.
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Done"].firstMatch.tap()
        sleep(2)
        element("overview-map", in: app)
            .coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .withOffset(CGVector(dx: -140, dy: -230))
            .press(forDuration: 1.2)
        let entry = element("add-site-sign-in", in: app)
        XCTAssertTrue(entry.waitForExistence(timeout: 20))
        XCTAssertEqual(entry.label, "Sign in to add a place pilots take off from.")
        XCTAssertFalse(element("add-site-here", in: app).exists)
        sleep(2)
        keepScreenshot("site-tiers-dark-4-entry-signed-out")
        entry.tap()
        XCTAssertTrue(
            app.textFields["Handle"].waitForExistence(timeout: 15)
                || element("account-google", in: app).exists
        )
        keepScreenshot("site-tiers-dark-5-entry-opens-the-account")
    }

    /// The two tiers' screens on the narrowest iPhone at a larger text
    /// size (the step above the largest of the standard sizes, about
    /// 135 %), light and dark: the filter's sheet with the switch, a place
    /// from pilots' page with the entry to confirm, the sheet to confirm
    /// in, the pilots who took off there, and the form for a new place.
    /// It keeps the screenshots; what it asserts is that each screen's
    /// parts are reached. Run it on the iPhone 17e
    /// (`ui-tests.sh -n 1 --first "iPhone 17e"`).
    func testThePlacesFromPilotsFitWithLargerText() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let account = try await makeAccount(on: server)
        for (dark, name) in [(false, "light"), (true, "dark")] {
            let app = launch(
                bundledMap: true, dark: dark,
                extra: [
                    "--server", server, "--map-at", Self.lechtal, "--all-sites",
                    "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL",
                ]
            )
            XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
            signIn(account, in: app)
            setLayer("layer-sites", on: true, in: app)

            // The filter's sheet, the switch at its foot off and on.
            let layer = openThePilotsSwitch(in: app)
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-1-filter-switch-off")
            set(layer, on: true)
            sleep(2)
            keepScreenshot("site-tiers-larger-text-\(name)-2-filter-switch-on")
            // Back to the sheet's head. A swipe down at its head puts the
            // sheet away, which is as good as "Done".
            for _ in 0..<2 where app.buttons["Done"].firstMatch.exists {
                app.swipeDown()
                usleep(500_000)
            }
            keepScreenshot("site-tiers-larger-text-\(name)-2b-filter-top")
            let done = app.buttons["Done"].firstMatch
            if done.waitForExistence(timeout: 3) {
                done.tap()
            }
            XCTAssertTrue(element("sites-filter-line", in: app).waitForExistence(timeout: 20))
            sleep(3)
            keepScreenshot("site-tiers-larger-text-\(name)-3-line-over-the-map")

            // The list and a place from pilots' page.
            layersRow("Flying sites", in: app).tap()
            let row = labelled(containing: "Beispieljoch", in: app)
            XCTAssertTrue(row.waitForExistence(timeout: 30))
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-4-list")
            row.tap()
            XCTAssertTrue(element("site-tier", in: app).waitForExistence(timeout: 20))
            let count = element("site-confirmations", in: app)
            XCTAssertTrue(count.waitForExistence(timeout: 20))
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-5-page-head")
            let confirm = bring("site-confirm", into: app)
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-6-page-confirm-entry")

            // The sheet to confirm in.
            confirm.tap()
            XCTAssertTrue(element("site-confirm-day", in: app).waitForExistence(timeout: 15))
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-7-confirm-sheet")
            _ = bring("site-confirm-send", into: app)
            keepScreenshot("site-tiers-larger-text-\(name)-7b-confirm-sheet-foot")
            app.buttons["Cancel"].firstMatch.tap()
            sleep(1)

            // The pilots who took off here.
            bring("site-confirmations", into: app, down: true).tap()
            XCTAssertTrue(element("site-confirmation", in: app).waitForExistence(timeout: 20))
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-8-pilots-who-took-off-here")
            app.navigationBars.buttons.firstMatch.tap()

            // The notice and what the core offers, at the page's foot.
            _ = bring("site-tier-note", into: app)
            keepScreenshot("site-tiers-larger-text-\(name)-9-page-notice")
            _ = bring("site-history", into: app)
            keepScreenshot("site-tiers-larger-text-\(name)-10-page-offers")
            app.navigationBars.buttons.firstMatch.tap()
            app.buttons["Done"].firstMatch.tap()
            sleep(2)

            // The form for a new place, empty, with its faults, and its foot.
            let entry = pressTheMap(dx: -140, dy: -230, in: app)
            sleep(2)
            keepScreenshot("site-tiers-larger-text-\(name)-11-entry-on-the-map")
            entry.tap()
            XCTAssertTrue(element("site-name", in: app).waitForExistence(timeout: 15))
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-12-form")
            bring("site-send", into: app).tap()
            XCTAssertTrue(element("site-fault", in: app).waitForExistence(timeout: 10))
            sleep(1)
            keepScreenshot("site-tiers-larger-text-\(name)-13-form-faults")
            for step in 1...4 {
                app.swipeUp()
                usleep(700_000)
                keepScreenshot("site-tiers-larger-text-\(name)-14-form-further-\(step)")
            }
            app.terminate()
        }
    }
}

/// What a pilot keeps on the account, against the development server with
/// an account the test makes: the kept take-offs, and the programs with
/// access.
extension VentiUITests {
    /// A take-off kept while signed in is on the account, and the note
    /// under it says so; a second start of the app, which begins with
    /// nothing kept and signed out, has it back from the account once it
    /// is signed in to it.
    func testKeptTakeoffsFollowTheAccount() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let account = try await makeAccount(on: server)
        let arguments = ["--server", server, "--map-at", "47.6653,11.7954,14"]
        // As on a new iPhone: a simulator on which an earlier test let
        // Venti use the position (`testAFlightRecordsItself` does) has
        // Today look around where that test left the iPhone, "near you",
        // and there the Jägerkamp is the card and no row.
        XCUIApplication().resetAuthorizationStatus(for: .location)
        let app = launch(bundledMap: true, extra: arguments)
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)

        // The Jägerkamp, one of Today's rows, kept on its page.
        // The settings are a page of the Today tab, and the sign-in left
        // the account's page open there: a second tap goes back to Today.
        openTab("Today", in: app)
        app.tabBars.buttons["Today"].tap()
        XCTAssertTrue(element("today-card", in: app).waitForExistence(timeout: 60))
        // Wherever the core's order puts it on the day: a row, or the card.
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier IN %@", ["today-row", "today-card"]))
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        bringUp(row, in: app)
        row.tap()
        let keep = element("site-keep", in: app)
        bringUp(keep, in: app)
        XCTAssertTrue(keep.waitForExistence(timeout: 10))
        keep.tap()
        XCTAssertTrue(app.buttons["Kept"].waitForExistence(timeout: 10))
        // Sent to the account at once: the core's note says where it lives.
        XCTAssertTrue(
            labelled(containing: "Kept on your Venti account and on this device", in: app)
                .waitForExistence(timeout: 30)
        )
        sleep(1)
        keepScreenshot("kept-account-1-site-kept")

        // A second start: every start of a UI test has an empty folder of
        // its own and is signed out, as a second iPhone would be.
        app.terminate()
        let second = launch(bundledMap: true, extra: arguments)
        XCTAssertTrue(second.buttons["Show my position"].waitForExistence(timeout: 90))
        openTab("Settings", in: second)
        let link = element("kept-sites-link", in: second)
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        link.tap()
        XCTAssertTrue(second.staticTexts["No take-offs of your own yet"].waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Kept on this device only", in: second).exists)

        // Signed in to the same account, the take-off comes back from it.
        signIn(account, in: second)
        openTab("Settings", in: second)
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        link.tap()
        let listed = second.descendants(matching: .any).matching(identifier: "kept-site-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(listed.waitForExistence(timeout: 30))
        XCTAssertTrue(
            labelled(containing: "Kept on your Venti account and on this device", in: second)
                .waitForExistence(timeout: 15)
        )
        sleep(1)
        keepScreenshot("kept-account-2-back-from-the-account")
    }

    /// Opens "Programs with access" from the account's page, signed in,
    /// and keeps a screenshot of the account's page on the way if asked.
    private func openPrograms(in app: XCUIApplication, keeping screenshot: String? = nil) {
        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        let link = bring("programs-link", into: app)
        if let screenshot {
            sleep(1)
            keepScreenshot(screenshot)
        }
        link.tap()
        XCTAssertTrue(app.navigationBars["Programs with access"].waitForExistence(timeout: 15))
    }

    /// A pilot gives "My logger" access with two things it may do, sees
    /// its secret once, finds it in the list, withdraws it after being
    /// asked, and the list is empty again.
    func testAPilotGivesAProgramAccessAndWithdrawsIt() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let account = try await makeAccount(on: server)
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)

        // The account's page has the row; a new account's list is empty.
        openPrograms(in: app, keeping: "programs-1-account")
        let empty = element("programs-empty", in: app)
        XCTAssertTrue(empty.waitForExistence(timeout: 30))
        XCTAssertEqual(empty.label, "No program has access to your account.")
        sleep(1)
        keepScreenshot("programs-2-empty")

        // The form: sent with nothing in it, it says what is missing.
        element("programs-give", in: app).tap()
        let name = element("token-name", in: app)
        XCTAssertTrue(name.waitForExistence(timeout: 15))
        sleep(1)
        keepScreenshot("programs-3-form")
        _ = bring("token-submit", into: app)
        element("token-submit", in: app).tap()
        let fault = bring("token-name-fault", into: app, down: true)
        XCTAssertTrue(fault.label.hasPrefix("Give the program a name"), fault.label)

        // A name, two things it may do, the password.
        name.tap()
        name.typeText("My logger\n")
        for scope in ["flights:write", "live:write"] {
            let row = bring("token-scope-\(scope)", into: app)
            row.tap()
            XCTAssertEqual(row.value as? String, "Chosen", scope)
        }
        sleep(1)
        keepScreenshot("programs-4-form-two-chosen")
        _ = bring("token-password", into: app)
        let password = app.secureTextFields["token-password"]
        password.tap()
        password.typeText(account.password + "\n")
        _ = bring("token-submit", into: app)
        sleep(1)
        keepScreenshot("programs-5-form-password")
        element("token-submit", in: app).tap()

        // The secret, this once, with the core's words; copied.
        let secret = element("token-secret", in: app)
        XCTAssertTrue(secret.waitForExistence(timeout: 30))
        // Not written into the test's log either: only its form is looked at.
        XCTAssertTrue(secret.label.hasPrefix("venti_pat_"))
        XCTAssertEqual(element("token-shown-once", in: app).label, "Copy it now: Venti does not show it again.")
        XCTAssertFalse(app.buttons["Cancel"].exists)
        let copy = element("token-copy", in: app)
        copy.tap()
        XCTAssertEqual(copy.value as? String, "Copied")
        sleep(1)
        keepScreenshot("programs-6-secret-once")
        let done = bring("token-done", into: app)
        XCTAssertEqual(done.label, "I have copied it")
        done.tap()

        // The list has it, with what it may do; the secret is nowhere.
        let listed = app.descendants(matching: .any).matching(identifier: "program-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "My logger")).firstMatch
        XCTAssertTrue(listed.waitForExistence(timeout: 15))
        XCTAssertTrue(listed.label.contains("Flights: write"), listed.label)
        XCTAssertTrue(listed.label.contains("Live tracking: send"), listed.label)
        XCTAssertFalse(secret.exists)
        XCTAssertFalse(labelled(containing: "venti_pat_", in: app).exists)
        XCTAssertFalse(empty.exists)
        sleep(1)
        keepScreenshot("programs-7-list")

        // Withdrawing asks first, in the core's words.
        let withdraw = bring("program-withdraw", into: app)
        withdraw.tap()
        let question = app.alerts.firstMatch
        XCTAssertTrue(question.waitForExistence(timeout: 10))
        XCTAssertTrue(question.label.contains("My logger"), question.label)
        XCTAssertTrue(question.buttons["Keep access"].exists)
        sleep(1)
        keepScreenshot("programs-8-withdraw-question")
        question.buttons["Withdraw access"].tap()
        XCTAssertTrue(empty.waitForExistence(timeout: 30))
        XCTAssertFalse(listed.exists)
        sleep(1)
        keepScreenshot("programs-9-empty-again")
    }

    /// The form that gives access, in the dark.
    func testTheAccessFormInTheDark() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let account = try await makeAccount(on: server)
        let app = launch(bundledMap: true, dark: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)
        openPrograms(in: app)
        XCTAssertTrue(element("programs-empty", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("programs-dark-1-empty")
        element("programs-give", in: app).tap()
        XCTAssertTrue(element("token-name", in: app).waitForExistence(timeout: 15))
        let row = bring("token-scope-flights:read", into: app)
        row.tap()
        sleep(1)
        keepScreenshot("programs-dark-2-form")
        _ = bring("token-submit", into: app)
        sleep(1)
        keepScreenshot("programs-dark-3-form-end")
    }
}

/// What public flights say about a site, and the places where pilots take
/// off and no site stands, against the development server's made-up
/// flights.
extension VentiUITests {
    /// The made-up place without a site in the Lechtal, the map's middle.
    private static let candidate = "47.2612,10.9312"

    func testPublicFlightsAtASiteAndTheTakeoffsWithoutASite() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let account = try await makeAccount(on: server)
        // The sample flight moved to the place: the pilot took off there.
        let app = launch(
            bundledMap: true,
            extra: [
                "--server", server, "--map-at", Self.candidate + ",12.5", "--all-sites",
                "--sample-flight-at", Self.candidate,
            ]
        )
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        signIn(account, in: app)
        setLayer("layer-sites", on: true, in: app)
        // What stands in the map's middle is the place: nothing over it.
        setLayer("layer-wind", on: false, in: app)
        setLayer("layer-pilots", on: false, in: app)

        // A take-off's page: the section under the confirmations.
        openMusterkogelWest(in: app)
        let section = bring("site-flights", into: app)
        XCTAssertEqual(section.label, "Public flights from here")
        let words = element("site-flights-words", in: app)
        XCTAssertTrue(words.waitForExistence(timeout: 20))
        XCTAssertTrue(words.label.contains("public flight"), words.label)
        let months = app.descendants(matching: .any).matching(identifier: "flown-month")
        if words.label.hasPrefix("No public flight") {
            XCTAssertEqual(months.count, 0)
        } else {
            XCTAssertEqual(months.count, 12)
            XCTAssertTrue(months.element(boundBy: 0).label.hasPrefix("January"), months.element(boundBy: 0).label)
        }
        sleep(1)
        keepScreenshot("flights-at-sites-1-site-section")
        if element("site-best-flight", in: app).exists {
            _ = bring("site-best-flight", into: app)
            keepScreenshot("flights-at-sites-2-best-flights")
        }
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Done"].firstMatch.tap()
        sleep(1)

        // The layer's switch beside "Places from pilots", off and on.
        let filter = layersRow("sites-filter", in: app, timeout: 30)
        XCTAssertTrue(filter.exists)
        filter.tap()
        let layer = bringInSheet("sites-candidates", in: app)
        _ = bringInSheet("sites-candidates-about", in: app)
        XCTAssertEqual(layer.label, "Take-offs without a site")
        XCTAssertFalse(isOn(layer))
        set(layer, on: true)
        sleep(1)
        keepScreenshot("flights-at-sites-3-filter-switch-on")
        app.buttons["Done"].firstMatch.tap()
        sleep(3)
        keepScreenshot("flights-at-sites-4-layer-on-the-map")

        // The ring in the map's middle: its sheet, with the offer to the
        // pilot who took off there.
        // The ring is drawn once Venti has answered: tapped again while
        // nothing opens.
        let place = element("candidate-words", in: app)
        for _ in 0..<4 where !place.exists {
            element("overview-map", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            _ = place.waitForExistence(timeout: 8)
        }
        XCTAssertTrue(place.waitForExistence(timeout: 10))
        XCTAssertTrue(place.label.hasPrefix("Pilots take off here"), place.label)
        sleep(2)
        keepScreenshot("flights-at-sites-5-candidate-sheet")
        // Under the place, in a list that has only the rows in view.
        let offer = bringInSheet("candidate-offer-words", in: app)
        XCTAssertTrue(offer.label.hasPrefix("You took off here on"), offer.label)
        _ = bringInSheet("candidate-set-aside", in: app)
        XCTAssertEqual(element("candidate-add", in: app).label, "Add this place")
        sleep(1)
        keepScreenshot("flights-at-sites-6-candidate-offer")
        // "Not this one": set aside, and the sheet closes.
        element("candidate-set-aside", in: app).tap()
        sleep(2)
        XCTAssertFalse(element("candidate-words", in: app).exists)
        app.terminate()

        // The section in the dark.
        let dark = launch(
            bundledMap: true, dark: true,
            extra: ["--server", server, "--map-at", Self.candidate + ",12.5", "--all-sites"]
        )
        XCTAssertTrue(dark.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("layer-sites", on: true, in: dark)
        openMusterkogelWest(in: dark)
        _ = bring("site-flights", into: dark)
        sleep(1)
        keepScreenshot("flights-at-sites-dark-site-section")
    }

    /// Opens the made-up take-off "Musterkogel West" from the list of the
    /// sites.
    private func openMusterkogelWest(in app: XCUIApplication) {
        let list = layersRow("Flying sites", in: app)
        XCTAssertTrue(list.exists)
        list.tap()
        let row = labelled(containing: "Musterkogel West", in: app)
        XCTAssertTrue(row.waitForExistence(timeout: 30))
        row.tap()
        XCTAssertTrue(element("site-plan-route", in: app).waitForExistence(timeout: 20))
    }
}
