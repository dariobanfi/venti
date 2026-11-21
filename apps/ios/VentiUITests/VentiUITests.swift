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
        XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 90))
        XCTAssertTrue(element("forecast-days", in: app).waitForExistence(timeout: 15))
        XCTAssertTrue(labelled(containing: "Not flyable", in: app).exists)
        XCTAssertTrue(element("forecast-model", in: app).exists)
        // Time for the squares to be drawn.
        sleep(4)
        keepScreenshot("map-forecast-1-today")

        // The second day. The stand-in dates its map from the development
        // server's clock, which the app names the days by, so they read
        // "Today" and "Tomorrow"; which day is today also hangs on the
        // simulator's time zone, so the test goes by the second segment.
        let tomorrow = element("forecast-days", in: app).buttons.element(boundBy: 1)
        XCTAssertTrue(tomorrow.waitForExistence(timeout: 15))
        tomorrow.tap()
        usleep(700_000)
        XCTAssertTrue(tomorrow.isSelected)
        XCTAssertTrue(element("forecast-key", in: app).exists)
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

    /// Chooses a layer of the forecast map's card by its chip.
    private func chooseForecastLayer(_ layer: String, in app: XCUIApplication) {
        let chip = element("forecast-layer-\(layer)", in: app)
        XCTAssertTrue(chip.waitForExistence(timeout: 15))
        for _ in 0..<3 where !isOnScreen(chip, in: app) {
            element("forecast-layers", in: app).swipeLeft()
        }
        chip.tap()
    }

    /// The forecast map by the hour, against the stand-in with a made-up
    /// day (`tools/made-up-day`): the card's row of layers, the line of
    /// hours, each of the five layers, the wind at a height, and a tapped
    /// square of an hour with its days side by side. The day is made up.
    /// The layer, the level and the chip are kept between launches: the
    /// test starts on "Thermals" at the ground and leaves it so, off.
    func testTheForecastMapShowsTheHoursOfADay() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        // Nothing over the squares: the wind, the sites and the pilots off.
        for layer in ["layer-wind", "layer-sites", "layer-pilots"] {
            setLayer(layer, on: false, in: app)
        }
        setLayer("forecast-chip", on: true, in: app)
        XCTAssertTrue(element("forecast-chip", in: app).isSelected)
        chooseForecastLayer("thermals", in: app)

        // The day as a whole, and the line of hours under the days.
        let strip = element("forecast-hours", in: app)
        XCTAssertTrue(strip.waitForExistence(timeout: 60))
        XCTAssertEqual(strip.value as? String, "The whole day")
        sleep(3)
        keepScreenshot("map-hours-1-thermals-day")

        // An hour, by a tap in the middle of the strip.
        strip.tap()
        XCTAssertNotEqual(strip.value as? String, "The whole day")
        sleep(3)
        keepScreenshot("map-hours-2-thermals-hour")
        // VoiceOver's swipe up: the next hour.
        let before = strip.value as? String
        strip.swipeRight()
        sleep(1)
        XCTAssertNotNil(before)

        // The other layers, each opening at the core's first hour.
        for (layer, name) in [("wind", "3-wind"), ("gusts", "4-gusts"), ("cloud", "5-cloud"), ("rain", "6-rain")] {
            chooseForecastLayer(layer, in: app)
            XCTAssertTrue(strip.waitForExistence(timeout: 60))
            XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 30))
            sleep(3)
            keepScreenshot("map-hours-\(name)")
        }

        // The wind at a height, from the level's menu.
        chooseForecastLayer("wind", in: app)
        let level = element("forecast-level", in: app)
        XCTAssertTrue(level.waitForExistence(timeout: 15))
        level.tap()
        let height = app.buttons["2000 m"]
        XCTAssertTrue(height.waitForExistence(timeout: 10))
        height.tap()
        XCTAssertTrue(strip.waitForExistence(timeout: 60))
        sleep(3)
        keepScreenshot("map-hours-7-wind-2000")

        // At 1000 m: where the made-up ground lies higher there is no
        // wind, and a tapped square says so. Squares are tapped across
        // the map until one says it; each opens a sheet.
        level.tap()
        let low = app.buttons["1000 m"]
        XCTAssertTrue(low.waitForExistence(timeout: 10))
        low.tap()
        XCTAssertTrue(strip.waitForExistence(timeout: 60))
        sleep(2)
        let sheet = element("forecast-hour-square", in: app)
        var none = false
        var kept = false
        search: for row in [0.3, 0.4, 0.2, 0.5] {
            for column in [0.5, 0.3, 0.7, 0.15, 0.85] {
                app.coordinate(withNormalizedOffset: CGVector(dx: column, dy: row)).tap()
                guard sheet.waitForExistence(timeout: 5) else { continue }
                let title = element("forecast-hour-title", in: app).label
                if !kept {
                    // The first square tapped: its days side by side.
                    XCTAssertTrue(element("forecast-square-days", in: app).waitForExistence(timeout: 10))
                    sleep(1)
                    keepScreenshot("map-hours-8-square")
                    kept = true
                }
                if title.contains("No wind at 1000 m") {
                    sleep(1)
                    keepScreenshot("map-hours-9-no-wind-at-1000")
                    none = true
                }
                app.buttons["Done"].tap()
                _ = sheet.waitForNonExistence(timeout: 5)
                if none {
                    break search
                }
            }
        }
        XCTAssertTrue(kept, "No tap opened a square of an hour.")
        XCTAssertTrue(none, "No square said that there is no wind at 1000 m.")

        // Back to "Thermals" at the ground, and off, as the next test
        // expects the map.
        level.tap()
        let ground = app.buttons["Ground"]
        if ground.waitForExistence(timeout: 10) {
            ground.tap()
        }
        chooseForecastLayer("thermals", in: app)
        for layer in ["layer-wind", "layer-sites", "layer-pilots"] {
            setLayer(layer, on: true, in: app)
        }
        setLayer("forecast-chip", on: false, in: app)
        XCTAssertFalse(element("forecast-chip", in: app).isSelected)
    }

    /// The keys of the forecast map's layers in the dark appearance, the
    /// card over the map, which stays light: the wind's, with its hours.
    func testTheForecastMapsKeysInTheDark() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(bundledMap: true, dark: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].waitForExistence(timeout: 90))
        setLayer("forecast-chip", on: true, in: app)
        for (layer, name) in [("wind", "wind"), ("cloud", "cloud"), ("thermals", "thermals")] {
            chooseForecastLayer(layer, in: app)
            XCTAssertTrue(element("forecast-hours", in: app).waitForExistence(timeout: 60))
            XCTAssertTrue(element("forecast-key", in: app).waitForExistence(timeout: 30))
            sleep(3)
            keepScreenshot("map-hours-dark-\(name)")
        }
        setLayer("forecast-chip", on: false, in: app)
        XCTAssertFalse(element("forecast-chip", in: app).isSelected)
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
}
