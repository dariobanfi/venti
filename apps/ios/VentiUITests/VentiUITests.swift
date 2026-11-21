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

    /// Switches one of the map tab's layer chips on or off. The row
    /// scrolls: a chip off the screen is brought into it first, by
    /// swiping the row on a chip that is on screen.
    private func setLayer(_ identifier: String, on: Bool, in app: XCUIApplication) {
        let chip = element(identifier, in: app)
        guard chip.waitForExistence(timeout: 10) else { return }
        for direction in [true, true, false, false, false, false] where !isOnScreen(chip, in: app) {
            let handle = ["layer-airspace", "layer-wind", "layer-sites", "forecast-chip"]
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

    /// Turns the cockpit to its next page: from the readings to the map,
    /// to the flight with what can be switched, and around.
    private func nextPage(in app: XCUIApplication) {
        let next = element("cockpit-next-page", in: app)
        XCTAssertTrue(next.waitForExistence(timeout: 15))
        next.tap()
        usleep(500_000)
    }

    /// Ends the session in the cockpit: its button is held, and nothing
    /// asks again.
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
        keepScreenshot("fly-3-glide")

        // The second page: the map as large as it gets, and under it the
        // way to the place flown to. The cockpit shows the way back to the
        // take-off until the pilot chooses a place; the waypoints are
        // listed nearest first.
        nextPage(in: app)
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
        nextPage(in: app)
        let lock = element("cockpit-lock", in: app)
        XCTAssertTrue(lock.waitForExistence(timeout: 10))
        XCTAssertTrue(element("Flight time", in: app).exists)
        XCTAssertTrue(element("cockpit-mute", in: app).exists)
        sleep(1)
        keepScreenshot("fly-5d-flight-page")

        // Locked, the screen answers no touch: a press where the button
        // for the next page is leaves the page where it is. Holding the
        // lock opens it.
        lock.tap()
        let unlock = element("cockpit-unlock", in: app)
        XCTAssertTrue(unlock.waitForExistence(timeout: 10))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.92)).tap()
        sleep(1)
        XCTAssertTrue(lock.exists)
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

        put(metresNorth: 0, altitude: 1600, speed: 0)
        open.tap()
        // The system asks whether Venti may use the position.
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow While Using App"]
        if allow.waitForExistence(timeout: 15) {
            allow.tap()
        }

        // Standing at the take-off: ready, and nothing is recorded.
        for _ in 0..<8 {
            put(metresNorth: 0, altitude: 1600, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(saying("Ready for take-off", in: app).waitForExistence(timeout: 20))
        XCTAssertTrue(saying("Recording starts by itself", in: app).exists)
        XCTAssertTrue(element("Altitude", in: app).exists)
        keepScreenshot("self-1-ready")
        // What can be switched before the take-off, with "Record now" for
        // a pilot who would rather not wait.
        nextPage(in: app)
        nextPage(in: app)
        XCTAssertTrue(element("cockpit-record-now", in: app).waitForExistence(timeout: 10))
        keepScreenshot("self-2-ready-flight-page")
        nextPage(in: app)

        // Forty-five seconds of flight: 10 m/s over the ground, sinking a
        // metre a second. The take-off is noticed within seconds.
        for second in 1...45 {
            put(metresNorth: 10 * Double(second), altitude: 1600 - Double(second), speed: 10)
            sleep(1)
            if second == 20 {
                XCTAssertTrue(saying("Recording", in: app).waitForExistence(timeout: 10))
                XCTAssertFalse(saying("Ready for take-off", in: app).exists)
                keepScreenshot("self-3-in-the-air")
            }
        }

        // Standing in the landing field: landed after twenty seconds, and
        // the cockpit says when the flight will be saved.
        for _ in 0..<40 {
            put(metresNorth: 450, altitude: 1555, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(saying("The flight is saved in", in: app).waitForExistence(timeout: 30))
        keepScreenshot("self-4-landed")

        // Two minutes after it came to rest the flight is in the logbook,
        // and its page is open.
        let details = app.buttons["Flight details"]
        for _ in 0..<150 where !details.exists {
            put(metresNorth: 450, altitude: 1555, speed: 0)
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

        // The zone is on the map tab.
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))

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
        XCTAssertTrue(labelled(containing: "Wind it works in", in: app).waitForExistence(timeout: 15))
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
        XCTAssertTrue(labelled(containing: "Wind it works in", in: app).waitForExistence(timeout: 15))

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
        let sites = app.buttons["Flying sites"]
        guard sites.waitForExistence(timeout: 60) else { return }
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
        // take-off. It names the hour, what is stronger and where, and
        // what the place itself has.
        let line = labelled(containing: "Much stronger near by at", in: app)
        // Whether the day Today opens on has it: the first with hours
        // left, which is the day a take-off's page speaks of.
        let isOnTheFirstDay = line.exists
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
            let lines = takeoffs.matching(NSPredicate(format: "label CONTAINS %@", "Much stronger near by at"))
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
                of: #"Much stronger near by at \d{1,2}:00: (gusts to|wind) \d+ km/h .+, \d+ km/h at the place"#,
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
        XCTAssertTrue(labelled(containing: "Wind it works in", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(element("site-status", in: app).waitForExistence(timeout: 30))
        // The page speaks of the first day that has hours left; where that
        // day has the line, it stands under the day's own.
        let onPage = element("site-nearby", in: app)
        if isOnTheFirstDay {
            XCTAssertTrue(onPage.waitForExistence(timeout: 10))
            XCTAssertTrue(onPage.label.contains("Much stronger near by at"), onPage.label)
        }
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

        // Under the picture, what the layers and the wind near by are and
        // are not.
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
        for layer in ["layer-wind", "layer-airspace", "layer-sites", "layer-pilots"] {
            setLayer(layer, on: true, in: app)
        }
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
        let others = ["layer-wind", "layer-airspace", "layer-sites", "layer-pilots"]
        for layer in others {
            setLayer(layer, on: false, in: app)
        }
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
    /// 46.3 to 48.5 N), their edge and the veil over what the radar did
    /// not see, in the part of the map the card leaves to be seen. The
    /// map that comes with the app ends long before: beyond it the
    /// picture lies on the map's bare ground.
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

    /// Waits until the radar's line on the card ends with these words,
    /// and says whether it came to that.
    private func radarSays(_ ending: String, in app: XCUIApplication, timeout: TimeInterval = 30) -> Bool {
        let words = element("radar-words", in: app)
        let until = Date().addingTimeInterval(timeout)
        while Date() < until {
            if words.exists, words.label.hasSuffix(ending) {
                return true
            }
            usleep(300_000)
        }
        return false
    }

    /// The rain radar on the map tab, against the development server,
    /// which has two frames recorded from DWD on 2026-11-21, of 11:35 and
    /// 11:40 UTC, a morning with little rain, and a radar's clock that
    /// stands at 11:44: the layer switched on, the card with both frames
    /// on the newest, the key and the notes behind their line, a step to
    /// the frame before, and the frames played to the newest again. The
    /// chip's choice is kept between launches: the test starts by looking
    /// which way it stands and switches it off at its end.
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
        // The card: both frames, on the newest, four minutes old by the
        // radar's clock. The clock's time is the simulator's time zone's.
        let strip = element("radar-frames", in: app)
        XCTAssertTrue(strip.waitForExistence(timeout: 60))
        XCTAssertTrue(radarSays(", 4 minutes ago", in: app, timeout: 60))
        XCTAssertTrue(element("radar-words", in: app).label.hasPrefix("Rain radar, "))
        XCTAssertEqual((strip.value as? String)?.hasSuffix(", 4 minutes ago"), true)
        XCTAssertTrue(element("radar-play", in: app).exists)
        // The card as it opens: the key and the notes are not on it.
        XCTAssertFalse(element("radar-key", in: app).exists)
        // The source's credit stands at the map while its picture does.
        XCTAssertTrue(labelled(containing: "Radar: Deutscher Wetterdienst", in: app).waitForExistence(timeout: 30))
        // Time for the picture to be drawn.
        sleep(4)
        keepScreenshot("map-radar-1-newest")

        // Behind "Key and notes": the key from the most rain down, what
        // the radar did not see, the note and the credit.
        showRadarKeyAndNotes(true, in: app)
        XCTAssertTrue(element("radar-key", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Cloudburst", in: app).exists)
        XCTAssertTrue(labelled(containing: "Not seen by the radar", in: app).exists)
        XCTAssertTrue(labelled(containing: "Datenbasis: Deutscher Wetterdienst", in: app).exists)
        sleep(1)
        keepScreenshot("map-radar-2-key-and-notes")
        showRadarKeyAndNotes(false, in: app)

        // The frame before: a tap on its cell, the strip's left half.
        strip.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.5)).tap()
        XCTAssertTrue(radarSays(", 9 minutes ago", in: app))
        XCTAssertEqual((strip.value as? String)?.hasSuffix(", 9 minutes ago"), true)
        sleep(2)
        keepScreenshot("map-radar-3-the-frame-before")

        // Played: through the frames, to stop at the newest.
        element("radar-play", in: app).tap()
        let played = radarSays(", 4 minutes ago", in: app)
        sleep(1)
        keepScreenshot("map-radar-4-played-to-the-newest")

        // Off again, as the next test expects the map.
        setLayer("radar-chip", on: false, in: app)
        XCTAssertFalse(chip.isSelected)
        XCTAssertFalse(element("radar-card", in: app).exists)
        XCTAssertTrue(played, "The frames did not play to the newest.")
    }

    /// The rain radar's card in the dark appearance, over the map, which
    /// stays light: closed and with "Key and notes" open, the map closer
    /// in, over the south of Bavaria, where a pixel of the picture is to
    /// be told from the next.
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

        let stations = app.buttons["Wind stations"]
        XCTAssertTrue(stations.waitForExistence(timeout: 90))
        // The credit appears with the first answer from the server.
        XCTAssertTrue(element("wind-credits", in: app).waitForExistence(timeout: 60))
        sleep(12)
        keepScreenshot("wind-1-map")

        // Who is in the air, from the same server: its development mode
        // has two gliders over the Tegernsee, one flying and one landed.
        let pilots = app.buttons["Pilots in the air"]
        XCTAssertTrue(pilots.waitForExistence(timeout: 30))
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
        XCTAssertTrue(stations.waitForExistence(timeout: 15))

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

        // What the pilots of the server share with everyone: on the
        // League tab. The server's development mode has public flights
        // of three made-up pilots.
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
        // mode's flights are all of 2026; every year together holds them
        // whatever year the test runs in.
        app.buttons["Ranking"].tap()
        let allYears = app.buttons["All years"]
        XCTAssertTrue(allYears.waitForExistence(timeout: 30))
        allYears.tap()
        XCTAssertTrue(element("ranked-pilot", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(labelled(containing: "Mara Muster", in: app).exists)
        XCTAssertTrue(labelled(containing: "best 3 flights shared with everyone", in: app).exists)
        sleep(1)
        keepScreenshot("community-1b-ranking")
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
        XCTAssertTrue(element("pilot-flight-row", in: app).waitForExistence(timeout: 30))
        sleep(1)
        keepScreenshot("community-3-pilot")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 20))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(everyone.waitForExistence(timeout: 15))
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

        // The same server knows the flying sites. They are drawn once the
        // map shows a valley and not a country: with the sample flight in
        // the logbook the map shows the valley it was flown in.
        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].waitForExistence(timeout: 30))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).waitForExistence(timeout: 15))
        openTab("Map", in: app)
        let sites = app.buttons["Flying sites"]
        XCTAssertTrue(sites.waitForExistence(timeout: 60))
        sleep(4)
        keepScreenshot("sites-1-map")

        // The signs on the map, as a list, and one of them.
        sites.tap()
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

        // Further down the site's page: what was flown from here, where
        // the server has public flights that took off at this site, and
        // where the site is from.
        let source = saying("OpenStreetMap", in: app)
        for _ in 0..<4 where !source.exists {
            app.swipeUp()
        }
        XCTAssertTrue(source.exists)
        if element("site-flight", in: app).exists {
            sleep(1)
            keepScreenshot("sites-3b-flown-from-here")
        }

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
        let plans = app.buttons["Flying plans"]
        XCTAssertTrue(plans.waitForExistence(timeout: 15))
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
        plans.tap()

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
    /// against a Venti server in development mode, whose two oldest
    /// public flights were flown from the same place on the same day.
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
        // Five flights, the newest first: the last is the oldest.
        let rows = app.descendants(matching: .any).matching(identifier: "public-flight-row")
        let oldest = rows.element(boundBy: 4)
        XCTAssertTrue(oldest.waitForExistence(timeout: 40))
        oldest.tap()
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
        XCTAssertTrue(aerial.waitForExistence(timeout: 30))
        XCTAssertTrue(aerial.isSelected)
        sleep(12)
        // The map tab names them in its own line of credits: the makers
        // of the photographs in view. This test names no Venti server, so
        // the style has no Austrian layer and basemap.at is not named.
        let credits = element("wind-credits", in: app).label
        XCTAssertTrue(credits.contains("Photos: ") && credits.contains("Bayerische Vermessungsverwaltung · EOX"), credits)
        keepScreenshot("online-4-map-aerial")
        aerial.tap()
        XCTAssertFalse(aerial.isSelected)
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
        let filter = element("sites-filter", in: app)
        XCTAssertTrue(filter.waitForExistence(timeout: 60))
        XCTAssertFalse(element("sites-filter-line", in: app).exists)
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
        // The button says that a filter is on, and which.
        XCTAssertEqual(filter.value as? String, "Take-offs for a wind from S")
        sleep(3)
        keepScreenshot("sites-filter-2-map")

        // The list: the ones that fit under what the filter leaves, then
        // the ones that say nothing under their heading and its sentence.
        app.buttons["Flying sites"].tap()
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
        _ = app.buttons["Flying sites"].waitForExistence(timeout: 60)
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
            XCTAssertTrue(app.buttons["Flying sites"].waitForExistence(timeout: 90))
            XCTAssertFalse(element("sites-filter-line", in: app).exists)
            sleep(4)
            keepScreenshot(name)
            app.terminate()
        }
    }
}
