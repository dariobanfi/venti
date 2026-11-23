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

    /// Sets the simulator's appearance, which the app follows where it was
    /// launched without `--dark`: a screen is seen in the dark without
    /// launching again and walking to it once more. The simulator keeps
    /// it, so the test's end sets it back to light whatever happened.
    private func setAppearance(dark: Bool) {
        XCUIDevice.shared.appearance = dark ? .dark : .light
        if dark, !isDarkUntilTheEnd {
            isDarkUntilTheEnd = true
            addTeardownBlock { @MainActor in
                XCUIDevice.shared.appearance = .light
            }
        }
        // The app draws anew in the other appearance.
        usleep(700_000)
    }

    private var isDarkUntilTheEnd = false

    /// A screenshot of what is on screen, light and then dark (see
    /// `setAppearance`), and light again after; light only where `dark`
    /// is `nil`.
    private func keepScreenshots(light: String, dark: String?) {
        keepScreenshot(light)
        guard let dark else { return }
        setAppearance(dark: true)
        keepScreenshot(dark)
        setAppearance(dark: false)
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
    /// chips on the map are what it shows, one at a time ("layer-wind",
    /// "what-thermals", "what-rain", "what-cloud"), and airspace
    /// ("layer-airspace") is its own round button too, which `setLayer`
    /// taps (`setAirspace`).
    private static let layersInTheSheet: Set<String> = [
        "layer-sites", "layer-my-flights", "layer-pilots", "layer-cables", "map-aerial", "layer-climbs",
        "layer-flown", "layer-thermal-map",
    ]

    /// Switches airspace on the map tab on or off with its round button,
    /// one tap: off until the pilot switches it on, and the choice is kept
    /// between launches, so a test looks which way it stands first.
    private func setAirspace(on: Bool, in app: XCUIApplication) {
        let button = app.buttons["map-airspace"]
        guard button.appears(timeout: 30) else {
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
        guard layers.appears(timeout: 30) else { return false }
        layers.tap()
        return app.navigationBars["Layers"].appears(timeout: 10)
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
        // The sheet has every layer as a switch and is longer than the
        // screen: the rows are under the switches, and a list makes a row
        // only once it is scrolled to.
        usleep(600_000)
        for _ in 0..<2 where !isOnScreen(row, in: app) {
            app.swipeUp()
            usleep(700_000)
        }
        _ = row.appears(timeout: timeout)
        if !isOnScreen(row, in: app) {
            app.swipeUp()
            usleep(700_000)
        }
        return row
    }

    /// A switch of the layers sheet, which is open: brought into the
    /// screen by swiping where it lies under the screen's lower edge.
    private func layersSwitch(_ identifier: String, in app: XCUIApplication) -> XCUIElement {
        let toggle = app.switches[identifier]
        _ = toggle.appears(timeout: 3)
        for _ in 0..<2 where !isOnScreen(toggle, in: app) {
            app.swipeUp()
            usleep(700_000)
        }
        return toggle
    }

    /// Waits until the map's credits, behind its credits button, name
    /// something: "Wind", "Sites". The credit comes with the first answer
    /// from the server, so this is how a test knows the map has it.
    @discardableResult
    private func waitForCredit(_ what: String, in app: XCUIApplication, timeout: TimeInterval = 60) -> Bool {
        let credits = app.buttons["map-credits"]
        guard credits.appears(timeout: timeout) else { return false }
        // A layer's credit is there only while the layer is: the chips'
        // choices are kept between launches, and a test before this one
        // may have left one off.
        if let chip = ["Wind": "layer-wind", "Sites": "layer-sites"][what] {
            setLayer(chip, on: true, in: app)
        }
        return waitUntil(timeout: timeout) { (credits.value as? String)?.contains(what) == true }
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
            let toggle = layersSwitch(identifier, in: app)
            if toggle.appears(timeout: 5) {
                set(toggle, on: on)
            }
            closeLayers(in: app)
            return
        }
        let chip = element(identifier, in: app)
        guard chip.appears(timeout: 10) else { return }
        // The row scrolls where the text is large: a chip off the screen
        // is brought into it first, by swiping the row on one that is on it.
        for direction in [true, true, false, false] where !isOnScreen(chip, in: app) {
            let handle = ["layer-wind", "what-thermals", "what-rain"]
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
        XCTAssertTrue(dots.appears(timeout: 15))
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
        XCTAssertTrue(dots.appears(timeout: 15))
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
        XCTAssertTrue(end.appears(timeout: 10))
        end.press(forDuration: 1.6)
    }

    /// The Fly tab is the cockpit: choosing it opens the cockpit, which
    /// waits for the take-off and asks for the position the first time.
    /// That is answered with `answer`, "Allow Once" unless a test says
    /// otherwise, so that the simulator keeps no answer for the tests
    /// after. On the Fly tab already, at rest after "End", the button
    /// opens it again. The tab bar lies under the cockpit once it is open,
    /// so the tab cannot be asked whether it is selected.
    private func openTheCockpit(in app: XCUIApplication, answer: String = "Allow Once") {
        let tab = app.tabBars.buttons["Fly"]
        let again = app.buttons["open-cockpit"]
        let end = element("cockpit-end", in: app)
        let question = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons[answer]
        XCTAssertTrue(tab.appears(timeout: 15))
        for _ in 0..<4 {
            if end.exists {
                return
            }
            for _ in 0..<40 where !(tab.exists && tab.isHittable) {
                usleep(250_000)
            }
            usleep(400_000)
            if again.exists, again.isHittable {
                again.tap()
            } else if tab.exists, tab.isHittable {
                tab.tap()
            }
            if question.appears(timeout: 3) {
                question.tap()
            }
            if end.appears(timeout: 10) {
                return
            }
        }
        XCTFail("The Fly tab did not open the cockpit.")
    }

    /// Opens the cockpit's settings: what the Fly tab listed before
    /// take-off, behind the one button on the cockpit's top line.
    private func openTheCockpitsSettings(in app: XCUIApplication) {
        if !element("cockpit-end", in: app).exists {
            openTheCockpit(in: app)
        }
        let settings = element("cockpit-settings", in: app)
        XCTAssertTrue(settings.appears(timeout: 15))
        settings.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 10))
    }

    /// The launch arguments of a replay of the sample flight from a moment
    /// of it (`LaunchOptions.replayFrom`), at twenty times its speed, held
    /// `seconds` of the flight later, or running on. The sample flight's
    /// legs (`venti_flight::testing::synthetic_flight`), in seconds of the
    /// flight: a glide to 90, the first thermal to 342 (nine circles of
    /// 28 s), a glide to 722 that leaves the sample task's start cylinder
    /// at about 390 and comes within 2 km of the sample zone at about
    /// 525, the second thermal to 890 (by the sample turnpoint west), two
    /// glides to 1520, and a spiral down to 1744.
    private static func replay(from start: Int, holdingAfter seconds: Int? = nil) -> [String] {
        ["--replay-from", "\(start)"] + (seconds.map { ["--replay-for", "\($0)"] } ?? [])
    }

    /// Replays the sample flight through the cockpit, from its settings,
    /// from the moment the launch named (`replay(from:holdingAfter:)`):
    /// with that the row is a button, where it is otherwise a menu that
    /// XCTest cannot tap over the cockpit in less than a minute a tap.
    private func replayTheSampleFlight(in app: XCUIApplication) {
        if !app.navigationBars["Settings"].exists {
            openTheCockpitsSettings(in: app)
        }
        let replay = app.buttons["replay-sample"]
        for _ in 0..<3 where !(replay.exists && isOnScreen(replay, in: app)) {
            app.swipeUp()
            usleep(500_000)
        }
        XCTAssertTrue(replay.appears(timeout: 10))
        replay.tap()
        XCTAssertTrue(element("Altitude", in: app).appears(timeout: 20))
    }

    /// Lets a held replay run on at twenty times its speed, or holds it.
    private func setReplay(running: Bool, in app: XCUIApplication) {
        choose(running ? "20 times faster" : "Paused", from: element("replay-speed", in: app), in: app)
        XCTAssertTrue(
            labelled(containing: running ? "Replay ×20" : "Replay paused", in: app).appears(timeout: 10)
        )
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
        guard reading.appears(timeout: 20) else { return nil }
        let said = "\(reading.label) \(reading.value.map { "\($0)" } ?? "")"
        guard let found = said.firstMatch(of: /(\d+) m over ground/) else { return nil }
        return Double(found.1)
    }

    private func put(
        metresNorth: Double, metresEast: Double = 0, altitude: Double, speed: Double, course: Double = 0
    ) {
        wander = wander == 0 ? 0.5 : 0
        let north = metresNorth + wander
        let east = metresEast / (111_195 * cos(47.664 * .pi / 180))
        XCUIDevice.shared.location = XCUILocation(
            location: CLLocation(
                coordinate: CLLocationCoordinate2D(latitude: 47.664 + north / 111_195, longitude: 11.772 + east),
                altitude: altitude, horizontalAccuracy: 5, verticalAccuracy: 5,
                course: course, speed: speed, timestamp: Date()
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
        XCTAssertTrue(element.appears(timeout: timeout))
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
        XCTAssertTrue(opener.appears(timeout: 15))
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
            XCTAssertTrue(opener.appears(timeout: 15))
            opener.tap()
            XCTAssertTrue(app.navigationBars[name].appears(timeout: 15))
            return
        }
        let tab = app.tabBars.buttons[name]
        XCTAssertTrue(tab.appears(timeout: 15))
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
    /// With `dark`, each screen also in the dark, under that prefix.
    private func addSampleFlightAndOpenIt(in app: XCUIApplication, prefix: String, dark: String? = nil) {
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        // Give the map a moment to draw.
        sleep(4)
        keepScreenshots(light: "\(prefix)-1-map", dark: dark.map { "\($0)-1-map" })

        openTab("Logbook", in: app)
        XCTAssertTrue(app.staticTexts["No flights yet"].appears(timeout: 15))
        keepScreenshots(light: "\(prefix)-2-logbook-empty", dark: dark.map { "\($0)-2-logbook-empty" })

        app.buttons["Add a sample flight"].tap()
        // A single imported flight opens straight away.
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Airtime", in: app).exists)
        sleep(6)
        keepScreenshots(light: "\(prefix)-3-flight", dark: dark.map { "\($0)-3-flight" })
    }

    /// From an empty logbook to the sample flight, light and dark; the
    /// flight replayed in the cockpit from its page, held and run on; its
    /// moments, details, thermals, the logbook and the statistics; and
    /// deleted again.
    func testAPilotImportsAFlightInspectsItAndDeletesIt() {
        let app = launch(bundledMap: true)
        addSampleFlightAndOpenIt(in: app, prefix: "light", dark: "dark")

        // A flight of the logbook once more, as the cockpit would have
        // shown it in the air.
        app.buttons["More"].tap()
        // A menu within the menu: its speeds unfold where it stood.
        tapWhenStill(app.buttons["Replay in the cockpit"])
        tapWhenStill(app.buttons["20 times faster"])
        XCTAssertTrue(element("Altitude", in: app).appears(timeout: 30))
        // The thermal's numbers and its picture, at one look: the thermal
        // is over a quarter of a minute after it began.
        XCTAssertTrue(waitForAll(["Gained", "lift-badge"], in: app, timeout: 60))
        keepScreenshot("dark-3b-replayed-in-the-cockpit")
        // A replay can be held at a moment, to look at it, and run on.
        choose("Paused", from: element("replay-speed", in: app), in: app)
        XCTAssertTrue(labelled(containing: "Replay paused", in: app).appears(timeout: 10))
        choose("10 times faster", from: element("replay-speed", in: app), in: app)
        XCTAssertTrue(labelled(containing: "Replay ×10", in: app).appears(timeout: 10))
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))
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
            _ = app.buttons["Flight details"].appears(timeout: 5)
        }
        XCTAssertTrue(app.buttons["Flight details"].exists)

        // Dragging along the altitude chart marks a moment, which stays.
        let chart = element("altitude-chart", in: app)
        XCTAssertTrue(chart.exists)
        chart.coordinate(withNormalizedOffset: CGVector(dx: 0.15, dy: 0.5))
            .press(forDuration: 0.3, thenDragTo: chart.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.5)))
        XCTAssertTrue(app.buttons["Show flight totals"].appears(timeout: 10))
        // The moment's numbers: with the ground under the flight, which
        // the region inside the app has, the height over it; until that
        // is read, the time in the air.
        XCTAssertTrue(element("Over ground", in: app).exists || element("In the air", in: app).exists)
        usleep(400_000)
        keepScreenshot("light-4-flight-moment")

        app.buttons["Show flight totals"].tap()
        XCTAssertTrue(labelled(containing: "Best climb", in: app).appears(timeout: 10))

        app.buttons["Flight details"].tap()
        XCTAssertTrue(app.navigationBars["Flight details"].appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("light-5-flight-details")

        // Choosing a thermal from the list marks it on the map.
        let thermal = element("thermal-row", in: app)
        for _ in 0..<4 where !thermal.exists {
            app.swipeUp()
        }
        XCTAssertTrue(thermal.appears(timeout: 15))
        keepScreenshot("light-5b-flight-thermals")
        thermal.tap()
        XCTAssertTrue(app.buttons["Show flight totals"].appears(timeout: 10))
        // The moment's numbers: with the ground under the flight, which
        // the region inside the app has, the height over it; until that
        // is read, the time in the air.
        XCTAssertTrue(element("Over ground", in: app).exists || element("In the air", in: app).exists)
        app.buttons["Show flight totals"].tap()

        // Back to the logbook, where the flight is now listed.
        app.navigationBars.buttons.firstMatch.tap()
        let row = element("logbook-row", in: app)
        XCTAssertTrue(row.appears(timeout: 15))
        usleep(400_000)
        keepScreenshots(light: "light-6-logbook", dark: "dark-4-logbook")

        // The totals lead to the statistics.
        element("logbook-totals", in: app).tap()
        XCTAssertTrue(app.staticTexts["Last twelve months"].appears(timeout: 15))
        XCTAssertTrue(element("statistics-year-2026", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("light-6b-statistics")
        // Further down: the months of the year, the typical flight, the
        // kinds of flight, the gliders and the bests. The list makes its
        // rows as they come into view.
        app.swipeUp()
        XCTAssertTrue(app.staticTexts["A typical flight"].appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("light-6c-statistics-typical")
        // The places flown from: the sample's one, without a name. Kept
        // as a screenshot when it comes into view; not a condition.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
            .press(forDuration: 0.2, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4)))
        if element("statistics-by-take-off", in: app).appears(timeout: 5) {
            usleep(400_000)
            keepScreenshot("light-6e-statistics-by-take-off")
        }
        app.swipeUp()
        XCTAssertTrue(app.staticTexts["Your bests"].appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("light-6d-statistics-bests")
        // At the end, the way to how the pilot flies. The sample logbook
        // has too few thermals for an observation, so the screen says so
        // (with observations: `testHowYouFlySaysWhatTheTracksShow`). Kept
        // when it is reached; not a condition.
        app.swipeUp()
        let howYouFly = element("statistics-how-you-fly", in: app)
        if howYouFly.appears(timeout: 5), howYouFly.isHittable {
            howYouFly.tap()
            let said = app.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier IN %@", ["how-you-fly-empty", "how-you-fly-card"]))
                .firstMatch
            if said.appears(timeout: 30) {
                usleep(400_000)
                keepScreenshot("light-6f-how-you-fly-empty")
            }
            if app.navigationBars["How you fly"].exists {
                app.navigationBars.buttons.firstMatch.tap()
                _ = app.staticTexts["Your bests"].appears(timeout: 10)
            }
        }
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(row.appears(timeout: 15))

        // The map tab now shows the flight too.
        openTab("Map", in: app)
        sleep(4)
        keepScreenshot("light-7-map-with-flight")
        openTab("Logbook", in: app)

        row.swipeLeft()
        app.buttons["Delete"].tap()
        let confirm = app.buttons["Delete flight"]
        XCTAssertTrue(confirm.appears(timeout: 10))
        keepScreenshot("light-8-delete")
        confirm.tap()
        XCTAssertTrue(app.staticTexts["No flights yet"].appears(timeout: 15))
    }

    /// "How you fly" with something to say: the logbook starts with the
    /// sample flight on five days (`--sample-logbook`), ten thermals, and
    /// the screen at the foot of the statistics shows its observations.
    func testHowYouFlySaysWhatTheTracksShow() {
        let app = launch(bundledMap: true, extra: ["--sample-logbook"])
        openTab("Logbook", in: app)
        let totals = element("logbook-totals", in: app)
        XCTAssertTrue(totals.appears(timeout: 30))
        totals.tap()
        XCTAssertTrue(app.staticTexts["Last twelve months"].appears(timeout: 15))

        // The way there is at the end of the statistics, whose list makes
        // its rows as they come into view.
        let howYouFly = element("statistics-how-you-fly", in: app)
        for _ in 0..<10 where !isOnScreen(howYouFly, in: app) {
            app.swipeUp()
            usleep(500_000)
        }
        XCTAssertTrue(isOnScreen(howYouFly, in: app))
        howYouFly.tap()
        XCTAssertTrue(app.navigationBars["How you fly"].appears(timeout: 15))
        // The core counts each flight from its file the first time.
        XCTAssertTrue(element("how-you-fly-card", in: app).appears(timeout: 60))
        XCTAssertFalse(element("how-you-fly-empty", in: app).exists)
        usleep(400_000)
        keepScreenshot("light-6f-how-you-fly")
    }

    /// The logbook by day: the sample flight on five days of July 2026 and
    /// twice on the 6th (`--sample-afternoon`), each day under its heading,
    /// the day of two with their count and airtime, the year's count above
    /// the first day. Light and dark, at the usual text size and larger.
    func testTheLogbookListsTheFlightsByDay() {
        for larger in Self.largerTextAsked ? [false, true] : [false] {
            let app = launch(
                bundledMap: true, extra: ["--sample-logbook", "--sample-afternoon"] + (larger ? Self.largerText : [])
            )
            openTab("Logbook", in: app)
            XCTAssertTrue(element("logbook-row", in: app).appears(timeout: 30))
            sleep(1)
            let size = larger ? "-larger-text" : ""
            keepScreenshots(light: "logbook-1-by-day\(size)", dark: "logbook-1-by-day-dark\(size)")
            XCTAssertTrue(element("logbook-year-2026", in: app).exists)
            // The newest day first, with its two flights counted.
            let newestDay = app.descendants(matching: .any).matching(identifier: "logbook-day").firstMatch
            XCTAssertTrue(newestDay.label.contains("2 flights"), newestDay.label)
            let newest = app.descendants(matching: .any).matching(identifier: "logbook-row").firstMatch
            XCTAssertTrue(newest.label.contains("Took off at"), newest.label)
            XCTAssertTrue(newest.label.contains("Sample Wing"), newest.label)
            XCTAssertTrue(newest.label.contains("Airtime"), newest.label)
            app.terminate()
        }
    }

    /// A flight of the logbook on its way to a league: what the pilot is
    /// told first, the file, XContest's page and the DHV-XC's form.
    /// Nothing is sent: the test signs in nowhere and never taps "Send".
    func testAFlightIsHandedToALeague() {
        let app = launch(bundledMap: true)
        addSampleFlightAndOpenIt(in: app, prefix: "league")

        // The flight's own button, under its profile.
        tapWhenStill(element("send-to-league", in: app))
        XCTAssertTrue(app.navigationBars["Send to a league"].appears(timeout: 15))

        // The sample flight is of 4 July 2026: the leagues take it until
        // the 18th, whether that is past or to come.
        XCTAssertTrue(element("league-deadline", in: app).appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "18 Jul 2026", in: app).exists)
        // Venti's own recorder made it, and signs nothing yet.
        XCTAssertTrue(labelled(containing: "does not sign its files yet", in: app).exists)

        // XContest: the file for the pilot, and the page to upload it on.
        XCTAssertTrue(element("league-file", in: app).appears(timeout: 15))
        XCTAssertTrue(element("xcontest-claim", in: app).exists)
        usleep(400_000)
        keepScreenshot("league-4-sheet")

        // The DHV-XC: nothing can be sent without a name and the upload
        // password.
        let send = element("dhvxc-send", in: app)
        for _ in 0..<4 where !send.isHittable {
            app.swipeUp()
        }
        XCTAssertTrue(send.appears(timeout: 10))
        XCTAssertFalse(send.isEnabled)
        usleep(400_000)
        keepScreenshot("league-5-dhvxc")

        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 15))
    }

    /// The cockpit, fed by a replay of the sample flight at twenty times
    /// its speed from ten seconds into its first thermal, which it then
    /// shows for twelve seconds, and held on the glide after it.
    func testTheCockpitShowsAReplayedFlight() {
        let app = launch(
            bundledMap: true, extra: ["--sample-waypoints"] + Self.replay(from: 100, holdingAfter: 320)
        )
        // The Fly tab opens on the cockpit, waiting for the take-off: the
        // thermal view beside the map is there on the ground too.
        openTheCockpit(in: app)
        XCTAssertTrue(element("thermal-view", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("fly-1-ready")

        // What was checked before take-off is behind the one settings
        // button. Who may watch a flight while it is under way: nobody,
        // until the pilot says otherwise, and nobody can be chosen
        // without an account to send the positions to.
        openTheCockpitsSettings(in: app)
        // The phone's tone is the core's to decide: no mute switch.
        XCTAssertTrue(element("vario-before-takeoff", in: app).exists)
        XCTAssertFalse(element("cockpit-sound", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-1a-settings")
        element("live-before-takeoff", in: app).tap()
        XCTAssertTrue(app.navigationBars["Live tracking"].appears(timeout: 10))
        XCTAssertTrue(saying("Nothing is sent", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("fly-1b-live")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 10))

        replayTheSampleFlight(in: app)

        // Circling in lift, the cockpit changes to its thermal numbers by
        // itself. Most of a circle later the map pictures the thermal: the
        // circles flown, moved with the wind, and where among them the
        // lift is. The sample waypoints hold one landing, well within
        // reach from up here: the cockpit names it with the height to
        // spare. All three at one look, within the thermal's twelve
        // seconds.
        XCTAssertTrue(waitForAll(["Gained", "lift-badge", "landing-badge"], in: app, timeout: 12))
        // The circles are pictured in the thermal view beside the map.
        XCTAssertTrue(element("thermal-view", in: app).exists)
        keepScreenshot("fly-2-thermal")

        // On the glide after the thermal it changes back, now with the wind
        // it worked out while circling, and holds there.
        XCTAssertTrue(element("Glide", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Replay paused", in: app).appears(timeout: 15))
        XCTAssertTrue(element("Wind", in: app).exists)
        // On a glide the thermal view stays beside the map, with the last
        // minutes of track and the wind it worked out.
        XCTAssertTrue(element("thermal-view", in: app).exists)
        XCTAssertTrue(element("thermal-view-wind", in: app).exists)
        // The direction flown, as an arrow and the core's compass word
        // under the ground speed.
        XCTAssertTrue(
            (element("cockpit-ground-speed", in: app).value as? String)?.contains("flying") == true
        )
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
        XCTAssertTrue(app.navigationBars["Fly to"].appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("fly-4-fly-to")
        labelled(containing: "Sample landing field", in: app).tap()
        XCTAssertTrue(valued(containing: "Sample landing field", in: app).appears(timeout: 10))
        sleep(2)
        keepScreenshot("fly-5-to-landing")

        // The wind of every band of height the glider circled in, from a
        // tap on the wind over the map.
        element("wind-badge", in: app).tap()
        XCTAssertTrue(app.navigationBars["Wind by height"].appears(timeout: 10))
        XCTAssertTrue(element("wind-by-height-row", in: app).appears(timeout: 10))
        // Over the bands, the wind the cockpit shows in the core's
        // words: how strong, where from, from what and how old.
        XCTAssertTrue(element("wind-words", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-5b-wind-by-height")
        app.buttons["Done"].tap()

        // The whole flight at a glance: the scored route, and the two
        // areas in which a turnpoint would make an FAI triangle of the
        // longest line flown.
        app.buttons["Show the whole flight"].tap()
        XCTAssertTrue(app.buttons["Follow the glider"].appears(timeout: 10))
        sleep(3)
        keepScreenshot("fly-5c-whole-flight")
        app.buttons["Follow the glider"].tap()
        XCTAssertTrue(app.buttons["Show the whole flight"].appears(timeout: 10))

        // The aerial view, at the layers button over the map. The
        // photographs need the network and may not come in CI: what is
        // kept is a screenshot of the button switched on, and the choice,
        // which every map shares, is switched off again. Not a condition.
        let aerial = element("cockpit-aerial", in: app)
        if aerial.appears(timeout: 5) {
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

        // The third page: how the flight stands. The vario's sound is no
        // button there: its switch is in the settings.
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Flight"))
        XCTAssertTrue(element("Flight time", in: app).appears(timeout: 10))
        XCTAssertFalse(element("cockpit-mute", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-5d-flight-page")

        // What the cockpit knows beyond its pages' readings, from the
        // third page: each a row in the core's words.
        element("cockpit-details", in: app).tap()
        XCTAssertTrue(app.navigationBars["Details"].appears(timeout: 10))
        XCTAssertTrue(element("cockpit-detail", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("fly-5d2-details")
        app.navigationBars["Details"].buttons["Done"].tap()
        XCTAssertTrue(element("cockpit-details", in: app).appears(timeout: 10))

        // Locked from the settings, the screen answers no touch: neither a
        // tap on the dots nor a swipe turns the page, and "End" ends
        // nothing. Holding the lock opens it.
        openTheCockpitsSettings(in: app)
        let lock = element("cockpit-lock", in: app)
        XCTAssertTrue(lock.appears(timeout: 10))
        lock.tap()
        let unlock = element("cockpit-unlock", in: app)
        XCTAssertTrue(unlock.appears(timeout: 10))
        element("cockpit-pages", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        app.swipeLeft()
        element("cockpit-end", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 1.6)
        sleep(1)
        XCTAssertTrue(unlock.exists)
        XCTAssertTrue((element("cockpit-pages", in: app).value as? String)?.hasPrefix("Flight") == true)
        keepScreenshot("fly-5e-locked")
        unlock.press(forDuration: 1.6)
        sleep(1)
        XCTAssertFalse(element("cockpit-unlock", in: app).exists)
        XCTAssertTrue(element("cockpit-end", in: app).exists)

        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))

        // Settings, and the credits the map data's licences ask for.
        openTab("Settings", in: app)
        XCTAssertTrue(app.staticTexts["Pilot"].appears(timeout: 15))
        keepScreenshot("settings-1")

        // The glider, the FAI triangle areas and the brightness, on a
        // page of their own: no class and no areas until the pilot says.
        let cockpit = element("cockpit-settings-link", in: app)
        for _ in 0..<3 where !(cockpit.exists && cockpit.isHittable) {
            app.swipeUp()
        }
        cockpit.tap()
        XCTAssertTrue(element("glider-class", in: app).appears(timeout: 15))
        XCTAssertTrue(isOn(app.switches["fai-areas"]) == false)
        usleep(400_000)
        keepScreenshot("settings-1b-cockpit")
        app.navigationBars.buttons.firstMatch.tap()
        for _ in 0..<3 where !app.buttons["Waypoints"].isHittable {
            app.swipeDown()
        }

        // The pilot's waypoints.
        app.buttons["Waypoints"].tap()
        XCTAssertTrue(labelled(containing: "Sample turnpoint", in: app).appears(timeout: 15))
        keepScreenshot("settings-3-waypoints")
        app.navigationBars.buttons.firstMatch.tap()

        let licences = app.buttons["Data and licences"]
        for _ in 0..<4 where !(licences.exists && licences.isHittable) {
            app.swipeUp()
        }
        app.buttons["Data and licences"].tap()
        XCTAssertTrue(app.staticTexts["Base map"].appears(timeout: 15))
        keepScreenshot("settings-2-credits")
    }

    /// A flight that records itself, with positions the test makes up:
    /// the cockpit waits on the ground and writes nothing, takes off when
    /// the iPhone moves as a glider does, and lands when it stops; ended
    /// then, the flight is in the logbook. Under a minute of flight: what
    /// is a take-off, a flight and a landing is the core's, whose tests
    /// hold its figures; this one sees that the shell hands the positions
    /// on and keeps what the core recorded. A flight that is not ended
    /// closes by itself two minutes after the landing, a rule of the core:
    /// no UI test waits for it.
    func testAFlightRecordsItself() {
        let app = XCUIApplication()
        app.resetAuthorizationStatus(for: .location)
        app.launchArguments = Self.naming(["--ui-testing", "--bundled-map"])
        continueAfterFailure = false
        app.launch()

        // The flight stands, flies and lands at the height the app's own
        // terrain has for the take-off: the core ends no flight while
        // known ground lies more than 50 m under the glider.
        let ground = Self.takeoffGroundM
        put(metresNorth: 0, altitude: ground, speed: 0)
        // The Fly tab opens the cockpit, and the system asks whether Venti
        // may use the position: for this run of the app only.
        openTheCockpit(in: app)

        // Standing at the take-off: ready, and nothing is recorded.
        for _ in 0..<3 {
            put(metresNorth: 0, altitude: ground, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(saying("Ready for take-off", in: app).appears(timeout: 20))
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
        XCTAssertTrue(element("cockpit-record-now", in: app).appears(timeout: 10))
        keepScreenshot("self-2-ready-flight-page")
        nextPage(in: app)

        // Forty-four seconds of flight at the take-off's height, 10 m/s
        // over the ground: 120 m out, a circle and a fifth of 20 s each
        // there, and back towards where it started. The take-off is
        // noticed within seconds; the circle is what shows the core it is
        // a flight and not a lift or a drive (a take-off is provisional
        // until then, and a provisional one that comes to rest is taken
        // back unsaved), and more than half a minute from the take-off to
        // coming to rest is what makes it more than a hop.
        let radius = 10 * 20 / (2 * Double.pi)
        for second in 1...44 {
            if second <= 12 {
                put(metresNorth: 10 * Double(second), altitude: ground, speed: 10)
            } else if second <= 36 {
                // Clockwise round a middle east of the line, entered going
                // north: the course is the angle turned.
                let turned = Double(second - 12) / 20 * 2 * Double.pi
                put(
                    metresNorth: 120 + radius * sin(turned), metresEast: radius * (1 - cos(turned)),
                    altitude: ground, speed: 10, course: (turned * 180 / Double.pi).truncatingRemainder(dividingBy: 360)
                )
            } else {
                put(metresNorth: 120 - 10 * Double(second - 36), altitude: ground, speed: 10, course: 180)
            }
            sleep(1)
            if second == 20 {
                XCTAssertTrue(saying("Recording", in: app).appears(timeout: 10))
                XCTAssertFalse(saying("Ready for take-off", in: app).exists)
                keepScreenshot("self-3-in-the-air")
            }
        }

        // Standing still, on the ground: landed after twenty seconds, and
        // the cockpit says when the flight will be saved.
        let landed = saying("The flight is saved in", in: app)
        for _ in 0..<45 where !landed.exists {
            put(metresNorth: 40, altitude: ground, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(landed.appears(timeout: 5))
        keepScreenshot("self-4-landed")

        // Ended now, the flight goes into the logbook as it would by
        // itself, and its page is open.
        endTheCockpit(in: app)
        let details = app.buttons["Flight details"]
        XCTAssertTrue(details.appears(timeout: 30))
        XCTAssertFalse(element("cockpit-end", in: app).exists)
        sleep(2)
        keepScreenshot("self-5-saved")
    }

    /// The positions the cockpit's recording holds, as its top line says
    /// them to VoiceOver; `nil` while it records nothing.
    private func recordedPositions(in app: XCUIApplication) -> Int? {
        let recording = element("cockpit-recording", in: app)
        guard recording.appears(timeout: 10),
              let found = "\(recording.value ?? "")".firstMatch(of: /(\d+) positions/)
        else { return nil }
        return Int(found.1)
    }

    /// A take-off seen while the app is open on another tab: the watch for
    /// it runs on every tab while the app is in front (`--watch-takeoff`;
    /// without it a test keeps the cockpit to the moment it opens it), and
    /// the take-off brings the Fly tab and its cockpit to the front,
    /// recording. Then the app goes to the back for twelve seconds while the
    /// glider flies on, and the recording has gone on with it.
    func testATakeoffOnAnotherTabOpensTheCockpitAndRecords() {
        let app = XCUIApplication()
        app.resetAuthorizationStatus(for: .location)
        app.launchArguments = Self.naming(["--ui-testing", "--bundled-map", "--watch-takeoff"])
        continueAfterFailure = false
        app.launch()
        let ground = Self.takeoffGroundM
        put(metresNorth: 0, altitude: ground, speed: 0)
        // The watch reads the position only where the pilot has allowed
        // it: once, in the cockpit, which is then ended on the ground.
        openTheCockpit(in: app)
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))

        // On the map tab, standing at the take-off: nothing opens.
        openTab("Map", in: app)
        for _ in 0..<4 {
            put(metresNorth: 0, altitude: ground, speed: 0)
            sleep(1)
        }
        let end = element("cockpit-end", in: app)
        XCTAssertFalse(end.exists)
        keepScreenshot("watch-1-map-standing")

        // Off at 10 m/s over the ground: the core sees the take-off, and
        // the cockpit opens over the map tab, recording.
        var metres = 0.0
        for _ in 0..<40 where !end.exists {
            metres += 10
            put(metresNorth: metres, altitude: ground, speed: 10)
            sleep(1)
        }
        XCTAssertTrue(end.appears(timeout: 10), "The take-off did not open the cockpit.")
        XCTAssertTrue(saying("Recording", in: app).appears(timeout: 10))
        keepScreenshot("watch-2-cockpit-opened-by-the-take-off")

        // To the back for twelve seconds, flying on: the positions still
        // reach the recording.
        let before = recordedPositions(in: app) ?? 0
        XCUIDevice.shared.press(.home)
        for _ in 0..<12 {
            metres += 10
            put(metresNorth: metres, altitude: ground, speed: 10)
            sleep(1)
        }
        app.activate()
        XCTAssertTrue(end.appears(timeout: 15))
        XCTAssertTrue(saying("Recording", in: app).exists)
        let after = recordedPositions(in: app) ?? 0
        XCTAssertGreaterThanOrEqual(
            after - before, 8, "Positions in the back: \(after - before) (\(before) before, \(after) after)."
        )
        usleep(400_000)
        keepScreenshot("watch-3-back-from-the-background")
    }

    /// The thermal map of thermal.kk7.ch on the map tab: off until it is
    /// switched on in the layers sheet, then its tiles over the Alps from
    /// the author's server (the network is needed) and its credit among
    /// the map's. Switched off again at the end: the choice is kept
    /// between launches.
    func testTheThermalMapOnTheMapTab() {
        let app = launch(bundledMap: true, extra: ["--map-at", "47.65,11.75,9"])
        let credits = app.buttons["map-credits"]
        XCTAssertTrue(credits.appears(timeout: 60))
        setLayer("layer-thermal-map", on: true, in: app)
        sleep(8)
        keepScreenshot("thermal-map-1-map-tab")
        XCTAssertTrue((credits.value as? String)?.contains("thermal.kk7.ch") == true)
        setLayer("layer-thermal-map", on: false, in: app)
        XCTAssertFalse((credits.value as? String)?.contains("thermal.kk7.ch") == true)
    }

    /// The cockpit at the text size above the largest standard one: on the
    /// ground waiting, its settings, circling and on a glide, the thermal
    /// view beside the map each time, and the details sheet, which is in
    /// the flight theme and so light; then "Glider and cockpit" in the
    /// dark. The sheet's list has `.contentMargins(.top, …)`: a grouped
    /// list without a heading otherwise begins with a heading's empty room
    /// under the bar.
    func testTheCockpitWithLargerText() {
        let app = launch(
            bundledMap: true,
            extra: ["--sample-waypoints"] + Self.largerText + Self.replay(from: 100, holdingAfter: 320)
        )
        openTheCockpit(in: app)
        XCTAssertTrue(element("thermal-view", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("larger-cockpit-1-ground")
        openTheCockpitsSettings(in: app)
        usleep(400_000)
        keepScreenshot("larger-cockpit-1b-settings")
        replayTheSampleFlight(in: app)
        XCTAssertTrue(waitForAll(["Gained", "lift-badge"], in: app, timeout: 12))
        keepScreenshot("larger-cockpit-2-circling")
        XCTAssertTrue(labelled(containing: "Replay paused", in: app).appears(timeout: 30))
        XCTAssertTrue(element("Glide", in: app).exists)
        XCTAssertTrue(element("thermal-view", in: app).exists)
        usleep(400_000)
        keepScreenshot("larger-cockpit-3-glide")
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Map"))
        XCTAssertTrue(swipePage(toNext: true, in: app).hasPrefix("Flight"))
        element("cockpit-details", in: app).tap()
        XCTAssertTrue(app.navigationBars["Details"].appears(timeout: 10))
        XCTAssertTrue(element("cockpit-detail", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("larger-cockpit-3-details")
        app.navigationBars["Details"].buttons["Done"].tap()
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))

        // "Glider and cockpit", a page of the app's settings, in the dark.
        setAppearance(dark: true)
        openTab("Settings", in: app)
        let cockpit = element("cockpit-settings-link", in: app)
        for _ in 0..<4 where !(cockpit.exists && cockpit.isHittable) {
            app.swipeUp()
        }
        cockpit.tap()
        XCTAssertTrue(element("glider-class", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("larger-dark-cockpit-1-glider-and-cockpit")
        app.swipeUp()
        usleep(400_000)
        keepScreenshot("larger-dark-cockpit-2-glider-and-cockpit-lower")
    }

    /// Airspace: a made-up control zone lies beside the end of the sample
    /// flight's long glide. Replayed from 440 s into the flight, held, the
    /// zone is some 3 km off; run on, the cockpit warns of it at about
    /// 525 s, and the replay is held again on the glide towards it.
    func testTheCockpitWarnsOfAirspace() {
        // "My flights" on the map tab for this launch alone (UserDefaults'
        // argument domain, nothing kept): the map then moves to the
        // sample flight once it is in the logbook, beside the zone.
        let app = launch(
            bundledMap: true,
            extra: ["--sample-airspace", "-VentiMapShowsMyFlights", "YES"] + Self.replay(from: 440, holdingAfter: 0)
        )

        // The map tab's airspace is off while the cockpit flies: its
        // warnings must not depend on what that map draws. (The choice is
        // kept between launches, and a test before may have left it on.)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        setAirspace(on: false, in: app)

        replayTheSampleFlight(in: app)

        // The zone is still some kilometres off, and the cockpit is
        // checking: no notice, no warning.
        XCTAssertTrue(labelled(containing: "Replay paused", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("fly-6a-before-the-zone")
        XCTAssertTrue(element("cockpit-end", in: app).exists, "The cockpit can be read while it replays.")
        XCTAssertFalse(element("airspace-notice", in: app).exists)
        XCTAssertFalse(element("airspace-banner", in: app).exists)

        // The airspace that matters next stands all the time, small over a
        // corner of the first page's map: the zone, already in reach from
        // the take-off, though no warning is up.
        let proximity = element("airspace-proximity", in: app)
        XCTAssertTrue(proximity.appears(timeout: 15))
        XCTAssertTrue(proximity.value.map { "\($0)" }?.contains("SAMPLE ZONE (NOT REAL)") == true)
        keepScreenshot("fly-8-airspace-proximity")

        // The warning comes when the glider is two kilometres from the
        // zone: run on, and held again once it is up.
        setReplay(running: true, in: app)
        let banner = element("airspace-banner", in: app)
        XCTAssertTrue(banner.appears(timeout: 30))
        setReplay(running: false, in: app)
        XCTAssertTrue(valued(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-6-airspace")

        // What lies on the line ahead, from the side: the zone as a block
        // from the ground up, the ground under it, and the glide running
        // into the one or the other, held while the glider glides towards
        // the zone. Its button is on the map's page.
        nextPage(in: app)
        app.buttons["Ahead, from the side"].tap()
        XCTAssertTrue(element("ahead-words", in: app).appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE", in: app).exists)
        // The ground is in the picture: the region that comes with the
        // app has its elevation, and the glide ends on it.
        XCTAssertTrue(element("ahead-ground", in: app).appears(timeout: 20))
        XCTAssertTrue(labelled(containing: "Your line", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-6b-airspace-ahead")
        app.buttons["Done"].tap()

        // On the map's page the same reading is large, under the map, with
        // its height line; the small one is not there.
        XCTAssertTrue(element("airspace-proximity-map", in: app).appears(timeout: 10))
        XCTAssertTrue(valued(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        XCTAssertFalse(element("airspace-proximity", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-8b-airspace-proximity-map")

        // On a slow machine a tap can fall into the moment the band is
        // drawn anew, and is lost: once more, then.
        banner.tap()
        let seen = app.buttons["Seen: quiet for 10 minutes"]
        if !seen.appears(timeout: 6) {
            banner.tap()
        }
        XCTAssertTrue(seen.appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "Class D control zone", in: app).exists)
        usleep(400_000)
        keepScreenshot("fly-7-airspace-warnings")
        // "Seen" would speak again as the glider comes closer; "does not
        // apply today" stays quiet, which is what this test can rely on.
        app.buttons["Does not apply today"].tap()
        XCTAssertTrue(element("Airspace warning, quieted", in: app).appears(timeout: 10))

        endTheCockpit(in: app)

        // The files behind the warnings.
        openTab("Settings", in: app)
        let files = app.buttons["Airspace"]
        XCTAssertTrue(files.appears(timeout: 15))
        files.tap()
        XCTAssertTrue(labelled(containing: "sample-zones-not-real", in: app).appears(timeout: 15))
        keepScreenshot("settings-4-airspace")

        // And the zone on the map: with the sample flight in the logbook
        // and "My flights" shown, the map shows the valley the flight
        // ends in, beside the zone.
        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 30))
        sleep(4)
        keepScreenshot("map-airspace-flight")

        // The flight's details say what it entered: the sample flight
        // clips the zone's edge in its second thermal.
        app.buttons["Flight details"].tap()
        let crossing = element("airspace-crossing-row", in: app)
        for _ in 0..<4 where !crossing.exists {
            app.swipeUp()
        }
        XCTAssertTrue(crossing.appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        usleep(400_000)
        keepScreenshot("flight-airspace")
        app.buttons["Done"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).appears(timeout: 15))
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
                if over.appears(timeout: 1.5) {
                    break search
                }
            }
        }
        XCTAssertTrue(over.exists, "No tap on the map opened what lies over a place.")
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE (NOT REAL)", in: app).exists)
        XCTAssertTrue(element("airspace-over-caveat", in: app).exists)
        usleep(400_000)
        keepScreenshot("airspace-over-1-place")
        // The foot of the sheet: the filter, in the core's words. Its
        // choice is kept between launches, so the test leaves it at all
        // heights.
        let filter = element("airspace-height-filter", in: app)
        for _ in 0..<4 where !isOnScreen(filter, in: app) {
            app.swipeUp()
        }
        XCTAssertTrue(filter.appears(timeout: 10))
        let lowest = filter.buttons.element(boundBy: filter.buttons.count - 1)
        lowest.tap()
        XCTAssertTrue(lowest.isSelected)
        usleep(400_000)
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
    /// reached. Light and dark.
    func testTheMapOpensWithoutAirspaceAndOneTapShowsIt() throws {
        // The made-up zone is a circle of one nautical mile around
        // 47.70 N, 11.733 E: at the map's middle.
        let near = launch(bundledMap: true, extra: ["--sample-airspace", "--map-at", "47.7,11.7333,12"])
        let button = near.buttons["map-airspace"]
        XCTAssertTrue(button.appears(timeout: 90))
        // The choice is kept between launches: an earlier run that
        // stopped half-way may have left it on.
        setAirspace(on: false, in: near)
        XCTAssertEqual(button.label, "Airspace")
        sleep(3)
        keepScreenshots(light: "map-airspace-1-off", dark: "map-airspace-1-off-dark")

        // Off: a tap on the zone opens no sheet.
        let over = element("airspace-over-words", in: near)
        near.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertFalse(over.appears(timeout: 3), "Airspace switched off opened a sheet.")

        // On with one tap: drawn, and a tap on it says what is over
        // the place.
        button.tap()
        usleep(700_000)
        XCTAssertTrue(button.isSelected)
        sleep(3)
        keepScreenshots(light: "map-airspace-2-on", dark: "map-airspace-2-on-dark")
        near.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(over.appears(timeout: 10), "The zone at the map's middle is not drawn.")
        XCTAssertTrue(labelled(containing: "SAMPLE ZONE (NOT REAL)", in: near).exists)
        near.buttons["Done"].tap()
        usleep(700_000)

        // The layers sheet's switch is the same choice.
        XCTAssertTrue(openLayers(in: near))
        let toggle = near.switches["layer-airspace"]
        XCTAssertTrue(toggle.appears(timeout: 5))
        XCTAssertTrue(isOn(toggle))
        set(toggle, on: false)
        closeLayers(in: near)
        XCTAssertFalse(button.isSelected, "The sheet's switch did not move the button.")
        near.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertFalse(over.appears(timeout: 3))

        // Too far out to draw, with Venti not reached: the wind's notice
        // says so, and no line asks to zoom in.
        let zoomIn = "zoom-in-notice"
        near.terminate()

        let away = launch(bundledMap: true, extra: ["--sample-airspace", "--map-at", "47.7,11.7333,5"])
        setAirspace(on: true, in: away)
        XCTAssertTrue(element("wind-notice", in: away).appears(timeout: 30))
        XCTAssertFalse(element(zoomIn, in: away).appears(timeout: 3))
        usleep(400_000)
        keepScreenshot("map-airspace-3-too-wide-not-reached")
        setAirspace(on: false, in: away)
        away.terminate()

        // With Venti: one line, airspace's while it is on, and none once
        // it is off: the gliders, climbs and flown thermals simply appear
        // when the map is close enough (owner decision 19).
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.7,11.7333,5"])
        setLayer("layer-pilots", on: true, in: app)
        setAirspace(on: true, in: app)
        let notice = element(zoomIn, in: app)
        XCTAssertTrue(notice.appears(timeout: 30))
        XCTAssertEqual(notice.label, "Zoom in to see airspace.")
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: zoomIn).count, 1)
        usleep(400_000)
        keepScreenshot("map-airspace-3-too-wide")
        setAirspace(on: false, in: app)
        XCTAssertTrue(notice.vanishes(timeout: 15))
        usleep(400_000)
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
        XCTAssertTrue(card.appears(timeout: 60))
        let status = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN %@", ["Flyable", "Marginal", "Not flyable"]))
            .firstMatch
        XCTAssertTrue(status.appears(timeout: 90))
        // Its readings, and the other take-offs as rows.
        XCTAssertTrue(labelled(containing: "Cloud base", in: app).appears(timeout: 30))
        XCTAssertTrue(element("today-row", in: app).exists)
        // Whose forecast it is, and that it is a reading of a model.
        XCTAssertTrue(labelled(containing: "the take-off decides", in: app).exists)
        sleep(2)
        keepScreenshot("today-1")

        // Another day.
        let days = app.buttons.matching(identifier: "today-days")
        if days.count > 1 {
            days.element(boundBy: days.count - 1).tap()
            usleep(400_000)
            keepScreenshot("today-2-another-day")
        }

        // A take-off's own page: its status again, and the two things to
        // do from here.
        card.tap()
        XCTAssertTrue(element("site-plan-route", in: app).appears(timeout: 15))
        XCTAssertTrue(element("site-status", in: app).appears(timeout: 30))
        XCTAssertTrue(app.buttons["Plan route"].exists)
        usleep(400_000)
        keepScreenshot("today-3-site")

        // "Start flight" brings the Fly tab to the front and opens the
        // cockpit, which waits for the take-off.
        app.buttons["Start flight"].tap()
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow While Using App"]
        if allow.appears(timeout: 10) {
            allow.tap()
        }
        XCTAssertTrue(element("cockpit-end", in: app).appears(timeout: 20))
        usleep(400_000)
        keepScreenshot("today-4-cockpit-from-the-site")
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))

        // The site's page is where it was.
        openTab("Today", in: app)
        XCTAssertTrue(app.buttons["Plan route"].appears(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(card.appears(timeout: 15))
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
        // Light and dark, then with the text size above the largest
        // standard one.
        for larger in Self.largerTextAsked ? [false, true] : [false] {
            let app = launch(bundledMap: true, extra: ["--server", server] + (larger ? Self.largerText : []))
            let suffix = larger ? "-larger" : ""
            openTab("Today", in: app)
            XCTAssertTrue(element("today-card", in: app).appears(timeout: 60))

            // The line, over the take-offs, in the core's words.
            let line = element("today-foehn", in: app)
            XCTAssertTrue(line.appears(timeout: 60))
            XCTAssertTrue(line.label.contains("Bolzano"), line.label)
            XCTAssertTrue(line.label.contains("Innsbruck"), line.label)
            XCTAssertLessThan(line.frame.minY, element("today-card", in: app).frame.minY)
            sleep(2)
            keepScreenshots(light: "foehn-1-today\(suffix)", dark: larger ? nil : "foehn-1-today-dark")

            // The view behind it: the first pair with its level, its words
            // and its chart.
            line.tap()
            XCTAssertTrue(app.navigationBars["Foehn"].appears(timeout: 15))
            XCTAssertTrue(element("foehn-row", in: app).appears(timeout: 15))
            XCTAssertTrue(labelled(containing: "Bolzano – Innsbruck", in: app).exists)
            XCTAssertTrue(element("foehn-level", in: app).exists)
            XCTAssertTrue(element("foehn-chart", in: app).appears(timeout: 10))
            usleep(400_000)
            keepScreenshots(light: "foehn-2-view\(suffix)", dark: larger ? nil : "foehn-2-view-dark")

            // The second pair, further down, and under the pairs what the
            // number means, the caution and whose data it is.
            let second = labelled(containing: "Klagenfurt – Salzburg", in: app)
            for _ in 0..<6 where !second.exists {
                app.swipeUp()
            }
            XCTAssertTrue(second.exists)
            usleep(400_000)
            keepScreenshots(light: "foehn-3-second-pair\(suffix)", dark: larger ? nil : "foehn-3-second-pair-dark")
            let credits = element("foehn-credits", in: app)
            for _ in 0..<8 where !credits.exists {
                app.swipeUp()
            }
            XCTAssertTrue(credits.exists)
            XCTAssertTrue(element("foehn-caution", in: app).exists)
            usleep(400_000)
            keepScreenshots(light: "foehn-4-what-it-means\(suffix)", dark: larger ? nil : "foehn-4-what-it-means-dark")
            element("foehn-done", in: app).tap()
            XCTAssertTrue(line.appears(timeout: 10))

            // The same view from the page's foot.
            let row = element("today-foehn-row", in: app)
            for _ in 0..<12 where !(row.exists && isOnScreen(row, in: app)) {
                app.swipeUp()
            }
            XCTAssertTrue(row.exists)
            usleep(400_000)
            keepScreenshots(light: "foehn-5-foot\(suffix)", dark: larger ? nil : "foehn-5-foot-dark")
            // Clear of the tab bar, which lies over the page's foot.
            if row.frame.maxY > app.windows.firstMatch.frame.maxY - 120 {
                app.swipeUp()
            }
            row.tap()
            XCTAssertTrue(element("foehn-row", in: app).appears(timeout: 15))
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
        XCTAssertTrue(element("today-card", in: app).appears(timeout: 60))
        let ownTitle = element("today-kept-title", in: app)
        XCTAssertFalse(ownTitle.exists)
        XCTAssertFalse(element("today-near-title", in: app).exists)

        // The Jägerkamp, one of the rows: its source names a fee and a
        // web page.
        let row = app.descendants(matching: .any).matching(identifier: "today-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(row.appears(timeout: 30))
        bringUp(row, in: app)
        row.tap()
        XCTAssertTrue(element("site-plan-route", in: app).appears(timeout: 15))

        // Further down, the button that keeps it.
        let keep = element("site-keep", in: app)
        bringUp(keep, in: app)
        XCTAssertTrue(keep.appears(timeout: 10))
        XCTAssertEqual(keep.label, "Keep as my take-off")
        keep.tap()
        let kept = app.buttons["Kept"]
        XCTAssertTrue(kept.appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "Kept on this device only", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("own-1-site-kept")

        // And what its source says besides: the fee in the core's
        // sentence, and the web page as a link that leaves the app.
        let page = element("site-website", in: app)
        bringUp(page, in: app)
        XCTAssertTrue(page.appears(timeout: 10))
        XCTAssertTrue(page.label.contains("www.dhv.de"), page.label)
        XCTAssertTrue(app.staticTexts["A fee is asked for using this site."].exists)
        usleep(400_000)
        keepScreenshot("own-2-site-more")

        // Back on Today it comes first, under the core's name for the
        // pilot's own, and the others under theirs.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(ownTitle.appears(timeout: 30))
        XCTAssertEqual(ownTitle.label, "Your take-offs")
        let nearTitle = element("today-near-title", in: app)
        XCTAssertTrue(nearTitle.appears(timeout: 10))
        XCTAssertEqual(nearTitle.label, "Within 30 km")
        let own = labelled(containing: "Jägerkamp", in: app)
        XCTAssertTrue(own.appears(timeout: 10))
        XCTAssertGreaterThan(own.frame.minY, ownTitle.frame.minY)
        XCTAssertLessThan(own.frame.minY, nearTitle.frame.minY)
        sleep(2)
        keepScreenshot("own-3-today")
        drawUp(in: app)
        usleep(400_000)
        keepScreenshot("own-4-today-the-others")

        // The settings list the pilot's own, with the core's note.
        app.buttons["Settings"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 15))
        let link = element("kept-sites-link", in: app)
        XCTAssertTrue(link.appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("own-5-settings")
        link.tap()
        let listed = app.descendants(matching: .any).matching(identifier: "kept-site-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(listed.appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Kept on this device only", in: app).exists)
        XCTAssertTrue(labelled(containing: "OpenStreetMap", in: app).exists)
        usleep(400_000)
        keepScreenshot("own-6-list")

        // Forgotten with a swipe: the list is empty and says what to do.
        listed.swipeLeft()
        XCTAssertTrue(app.buttons["Forget"].appears(timeout: 10))
        app.buttons["Forget"].tap()
        XCTAssertTrue(app.staticTexts["No take-offs of your own yet"].appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("own-7-list-empty")

        // And Today is as it was: no groups.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("today-card", in: app).appears(timeout: 30))
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
        guard sites.appears(timeout: 30) else { return }
        sites.tap()
        let ramp = app.descendants(matching: .any).matching(identifier: "site-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Take-off", "47.665"))
            .firstMatch
        guard ramp.appears(timeout: 15) else { return }
        bringUp(ramp, in: app)
        ramp.tap()
        guard app.buttons["Plan route"].appears(timeout: 15) else { return }
        let description = element("site-description", in: app)
        bringUp(description, in: app)
        if description.exists {
            usleep(400_000)
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
        XCTAssertTrue(card.appears(timeout: 60))
        let status = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label IN %@", ["Flyable", "Marginal", "Not flyable"]))
            .firstMatch
        XCTAssertTrue(status.appears(timeout: 90))
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
        XCTAssertTrue(line.appears(timeout: 10))
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
        usleep(400_000)
        keepScreenshot("air-1-today-the-days-line")

        // The take-off the line is about: its own page.
        line.tap()
        XCTAssertTrue(element("site-plan-route", in: app).appears(timeout: 15))
        XCTAssertTrue(element("site-status", in: app).appears(timeout: 30))
        // The page speaks of the day chosen on Today: the line stands
        // under the day's own.
        let onPage = element("site-nearby", in: app)
        XCTAssertTrue(onPage.appears(timeout: 10))
        XCTAssertTrue(onPage.label.contains("Much stronger near by at"), onPage.label)
        usleep(400_000)
        keepScreenshot("air-2-site-the-days-line")

        // Further down, the picture of the day's air, with the hours to
        // tap over it.
        let chart = element("air-chart", in: app)
        for _ in 0..<4 where !chart.exists {
            app.swipeUp()
        }
        XCTAssertTrue(chart.appears(timeout: 15))
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
        XCTAssertTrue(detail.appears(timeout: 10))
        usleep(400_000)
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
        XCTAssertTrue(figure.firstMatch.appears(timeout: 10))
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
        usleep(400_000)
        keepScreenshot("air-4-an-hour-in-words")

        // At the page's end, behind "How this is read": what the layers
        // and the wind near by are and are not.
        unfoldHowASiteIsRead(in: app)
        let layersNote = labelled(containing: "Layers are read between the model's levels", in: app)
        bringUp(layersNote, in: app)
        XCTAssertTrue(layersNote.appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "Near by is the strongest wind and gust", in: app).exists)
        usleep(400_000)
        keepScreenshot("air-5-the-notes")

        // The picture once more in the dark, on the day before, whose air
        // has an inversion at the ground under its stable layer: the bands
        // are in the dark theme's muted ink.
        app.terminate()
        app.launchArguments = [
            "--ui-testing", "--bundled-map", "--dark", "--server", server, "--today-at", "2026-11-21T18:00:00Z",
        ]
        app.launch()
        XCTAssertTrue(card.appears(timeout: 60))
        XCTAssertTrue(status.appears(timeout: 90))
        sleep(4)
        card.tap()
        XCTAssertTrue(element("site-status", in: app).appears(timeout: 30))
        for _ in 0..<4 where !chart.exists {
            app.swipeUp()
        }
        XCTAssertTrue(chart.appears(timeout: 15))
        let inTheDark = hours.allElementsBoundByIndex.map { ($0.value as? String) ?? "" }
        print("The hours of the picture in the dark: \(inTheDark)")
        XCTAssertTrue(inTheDark.contains { $0.contains("Inversion between") }, "\(inTheDark)")
        if let inverted = inTheDark.firstIndex(where: { $0.contains("Inversion between") }) {
            hours.element(boundBy: inverted).tap()
            usleep(600_000)
        }
        bringUp(detail, in: app)
        usleep(400_000)
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
        XCTAssertTrue(row.appears(timeout: 10))
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
        XCTAssertTrue(days.firstMatch.appears(timeout: 90))
        // Every forecast read: the days are those of the longest.
        XCTAssertTrue(element("today-card", in: app).appears(timeout: 60))
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
        XCTAssertTrue(card.appears(timeout: 60))
        let kind = element("today-kind-of-day", in: app)
        XCTAssertTrue(kind.appears(timeout: 90))
        XCTAssertEqual(kind.label, "Big day")
        XCTAssertTrue(labelled(containing: "m/s to a cloud base of 3500 m", in: app).appears(timeout: 15))
        sleep(2)
        keepScreenshot("today-5-big-day")

        // Further down: the hours with their climbs, and the other
        // take-offs with their kinds of day.
        app.swipeUp()
        usleep(400_000)
        keepScreenshot("today-6-kinds-of-day")

        // The take-off's own page says it too.
        app.swipeDown()
        XCTAssertTrue(card.appears(timeout: 15))
        card.tap()
        XCTAssertTrue(element("site-status", in: app).appears(timeout: 30))
        XCTAssertTrue(element("thermals", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("today-7-site-on-a-big-day")

        // Further down it draws the day's air: the thermals hour by hour
        // and the wind at heights.
        let chart = element("air-chart", in: app)
        for _ in 0..<4 where !chart.exists {
            app.swipeUp()
        }
        XCTAssertTrue(chart.appears(timeout: 15))
        // Its head is on screen already: the page is drawn up gently, not
        // flung, until the whole picture stands clear of the tab bar.
        let bottom = app.windows.firstMatch.frame.maxY
        for _ in 0..<4 where chart.frame.maxY > bottom - 120 {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.2, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.45)))
            sleep(1)
        }
        usleep(400_000)
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
            if table.appears(timeout: 60) {
                // The rows of thermals are the table's last: the table's
                // hours scroll sideways, its rows stand in the page.
                _ = element("forecast-thermals-note", in: app).appears(timeout: 10)
                usleep(400_000)
                keepScreenshot("forecast-thermals")
            }
        }
    }

    /// The forecast map on the map tab: the kind of day square by square,
    /// against the stand-in server with a made-up map (`tools/made-up-day`).
    /// The map is made up; what is tested is that "Thermals" shows the day
    /// as a whole on the panel's line of time, its key behind the "i", and
    /// what a square says when one is tapped.
    func testTheMapShowsTheKindOfDaySquareBySquare() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))

        // The choices are kept between launches: an earlier run that
        // stopped half-way may have left the layers off.
        for layer in ["layer-sites", "layer-pilots"] {
            setLayer(layer, on: true, in: app)
        }
        setLayer("layer-airspace", on: false, in: app)
        showWhat("what-thermals", in: app)

        // The panel at the map's foot: the line of time with the days as a
        // whole, the line in words, the key; the whole key and the notes
        // are not on it.
        XCTAssertTrue(timeCells(in: app).firstMatch.appears(timeout: 90))
        let line = element("what-line", in: app)
        XCTAssertTrue(waitUntil(timeout: 60) { line.label.hasPrefix("Thermals, ") && line.label.contains(", forecast, ") })
        XCTAssertEqual(chosenTime(in: app)?.contains(", ") == false, true, "The thermals open on the day as a whole.")
        XCTAssertFalse(element("what-key-whole", in: app).exists)
        // Time for the squares to be drawn.
        sleep(4)
        keepScreenshot("map-forecast-1-today")
        // Behind the "i": the whole key, the model and its run.
        showWhatsKeyAndNotes(true, in: app)
        XCTAssertTrue(element("what-key-whole", in: app).appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Not flyable", in: app).exists)
        XCTAssertTrue(element("what-notes", in: app).exists)
        usleep(400_000)
        keepScreenshot("map-forecast-1b-key-and-notes")
        showWhatsKeyAndNotes(false, in: app)

        // The second day as a whole: the line's second cell without an hour.
        let days = timeCells(in: app).matching(NSPredicate(format: "NOT (label CONTAINS ', ')"))
        let tomorrow = days.element(boundBy: 1)
        XCTAssertTrue(tomorrow.appears(timeout: 15))
        let tomorrowName = tomorrow.label
        tomorrow.tap()
        usleep(700_000)
        XCTAssertTrue(tomorrow.isSelected)
        XCTAssertTrue(line.label.contains(tomorrowName), line.label)
        sleep(3)
        keepScreenshot("map-forecast-2-tomorrow")

        // A square, tapped. The sites and the pilots are switched off
        // first, since a tap on one of them opens its page instead (the
        // development server's glider flies over the made-up map's middle);
        // they are switched on again before anything here may fail. The
        // wind stations are not drawn while the thermals are shown. The
        // tap is a little above the map's middle, where the made-up map
        // has squares (47.45 to 47.95 N, 11.45 to 12.35 E). Only a square's
        // sheet is checked, and only when it opens.
        for layer in ["layer-sites", "layer-pilots"] {
            setLayer(layer, on: false, in: app)
        }
        sleep(2)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4)).tap()
        let square = element("forecast-square", in: app)
        var status: String?
        /// Which of the forecast's days was chosen when it opened, counted
        /// from the first.
        var forecastOpenedOn: Int?
        if square.appears(timeout: 15) {
            let said = element("forecast-square-status", in: app)
            if said.appears(timeout: 10) {
                status = said.label
            }
            usleep(400_000)
            keepScreenshot("map-forecast-3-square")
            // The forecast for the square's place, kept when it comes. It
            // opens on the day the map showed, the second: the second of
            // the forecast's days too, since both begin today.
            let toForecast = element("forecast-square-forecast", in: app)
            if toForecast.exists, toForecast.isHittable {
                toForecast.tap()
                if element("forecast-table", in: app).appears(timeout: 60) {
                    let days = element("forecast-day", in: app).buttons
                    if days.firstMatch.appears(timeout: 10) {
                        forecastOpenedOn = (0..<days.count).first { days.element(boundBy: $0).isSelected }
                    }
                    usleep(400_000)
                    keepScreenshot("map-forecast-4-forecast")
                }
                // Back to the square's sheet, which has the button to close it.
                for _ in 0..<2 where !app.buttons["Done"].appears(timeout: 5) {
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
        for layer in ["layer-sites", "layer-pilots"] {
            setLayer(layer, on: true, in: app)
        }
        // The square's status, as a word the core gives.
        if let status {
            XCTAssertTrue(["Flyable", "Marginal", "Not flyable"].contains(status), status)
        }

        // Back to the wind, as the next test expects the map.
        showWhat("layer-wind", in: app)
        XCTAssertFalse(element("what-thermals", in: app).isSelected)
        // The square's forecast, on the map's day: tomorrow. Asked last,
        // so that a failure leaves the layers as the next test expects.
        if let forecastOpenedOn {
            XCTAssertEqual(forecastOpenedOn, 1, "The square's forecast did not open on the map's second day.")
        }
    }

    /// Where the map opens for the screenshots of the hours: the made-up
    /// map's squares (47.45 to 47.95 N, 11.45 to 12.35 E, nine by five),
    /// all of them in the part of the map the panel leaves to be seen.
    private static let overTheMadeUpSquares = "47.44,11.9,8.1"

    /// Chooses what the map shows by its chip, one at a time ("layer-wind",
    /// "what-thermals", "what-rain", "what-cloud"): the chosen one stays
    /// chosen, another is chosen.
    private func showWhat(_ chip: String, in app: XCUIApplication) {
        setLayer(chip, on: true, in: app)
        XCTAssertTrue(element(chip, in: app).isSelected, chip)
    }

    /// The cells of the panel's line of time, each a button labelled with
    /// its time: "Today, 13:00", a day's name, "Now", a radar's clock.
    private func timeCells(in app: XCUIApplication) -> XCUIElementQuery {
        app.buttons.matching(identifier: "map-time-cell")
    }

    /// The label of the cell the line stands on.
    private func chosenTime(in app: XCUIApplication) -> String? {
        let cells = timeCells(in: app)
        return (0..<cells.count).map { cells.element(boundBy: $0) }.first { $0.isSelected }?.label
    }

    /// Chooses the first cell of the line whose time ends so (", 13:00"),
    /// the first day's that has it (today's line begins at the hour that
    /// has begun, so on an evening it is tomorrow's), and says whether it
    /// came to that. The
    /// line scrolls sideways: the cell is brought in by swiping it.
    @discardableResult
    private func chooseTime(endingWith time: String, in app: XCUIApplication) -> Bool {
        let cell = timeCells(in: app).matching(NSPredicate(format: "label ENDSWITH %@", time)).firstMatch
        guard cell.appears(timeout: 60) else { return false }
        let strip = element("map-time", in: app)
        for _ in 0..<6 where !isOnScreen(cell, in: app) || !strip.frame.insetBy(dx: -1, dy: -1).contains(cell.frame) {
            if cell.frame.midX > strip.frame.midX {
                strip.swipeLeft()
            } else {
                strip.swipeRight()
            }
            usleep(500_000)
        }
        cell.tap()
        usleep(600_000)
        return cell.isSelected
    }

    /// Waits until something holds, and says whether it came to that.
    private func waitUntil(timeout: TimeInterval, _ holds: () -> Bool) -> Bool {
        let until = Date().addingTimeInterval(timeout)
        while Date() < until {
            if holds() {
                return true
            }
            usleep(300_000)
        }
        return holds()
    }

    /// Opens or closes what stands behind the panel's "i".
    private func showWhatsKeyAndNotes(_ shown: Bool, in app: XCUIApplication) {
        let more = element("what-more", in: app)
        XCTAssertTrue(more.appears(timeout: 15))
        if (more.value as? String == "Shown") != shown {
            more.tap()
            usleep(600_000)
        }
        XCTAssertEqual(more.value as? String, shown ? "Shown" : "Hidden")
    }

    /// Chooses the height of the forecast's wind from its menu.
    private func chooseForecastLevel(_ name: String, in app: XCUIApplication) {
        let level = element("forecast-level", in: app)
        XCTAssertTrue(level.appears(timeout: 15))
        level.tap()
        let height = app.buttons[name]
        XCTAssertTrue(height.appears(timeout: 10))
        height.tap()
        usleep(700_000)
    }

    /// The forecast by the hour, against the stand-in with a made-up day
    /// (`tools/made-up-day`): the thermals' day as a whole and at 13:00,
    /// the wind, its gusts, the cloud and the rain at 13:00 on the line of
    /// time, the wind at a height, the key and the notes behind the "i",
    /// and a tapped square of an hour with its days side by side; the
    /// panel over the map, which stays light, in the dark too. The day
    /// is made up. The map opens over the made-up squares, all of them
    /// above the panel. What the map shows, the gusts and the level are
    /// kept between launches: the test leaves the wind at the ground, its
    /// mean.
    func testTheForecastMapShowsTheHoursOfADay() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_MADE_UP_DAY_SERVER"], !server.isEmpty else {
            throw XCTSkip("No stand-in server with a made-up day was named in VENTI_MADE_UP_DAY_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", Self.overTheMadeUpSquares])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        // Nothing over the squares: the airspace, the sites and the pilots
        // off.
        let others = ["layer-sites", "layer-pilots"]
        for layer in others {
            setLayer(layer, on: false, in: app)
        }
        setLayer("layer-airspace", on: false, in: app)
        showWhat("what-thermals", in: app)

        // The day as a whole, and the line of time.
        let line = element("what-line", in: app)
        XCTAssertTrue(timeCells(in: app).firstMatch.appears(timeout: 60))
        XCTAssertTrue(waitUntil(timeout: 60) { line.label.hasPrefix("Thermals, ") })
        XCTAssertFalse(element("what-key-whole", in: app).exists)
        XCTAssertFalse(element("forecast-level", in: app).exists)
        sleep(3)
        keepScreenshot("map-hours-1-thermals-day")

        // An hour, by a tap on the line: one o'clock.
        XCTAssertTrue(chooseTime(endingWith: ", 13:00", in: app), "The line stands on \(chosenTime(in: app) ?? "")")
        XCTAssertTrue(waitUntil(timeout: 30) { line.label.hasPrefix("Thermals, ") && line.label.contains("13:00, forecast") }, line.label)
        sleep(3)
        keepScreenshots(light: "map-hours-2-thermals-hour", dark: "map-hours-dark-thermals")

        // The wind: on what the stations measure now, then at one o'clock.
        showWhat("layer-wind", in: app)
        XCTAssertTrue(waitUntil(timeout: 30) { line.label == "Wind, now, measured" }, line.label)
        sleep(2)
        keepScreenshot("map-hours-3a-wind-measured")
        XCTAssertTrue(chooseTime(endingWith: ", 13:00", in: app))
        XCTAssertTrue(waitUntil(timeout: 30) { line.label.hasPrefix("Wind at the ground, ") && line.label.contains("13:00, forecast") }, line.label)
        // The height's menu and the gusts with the wind's forecast alone.
        XCTAssertTrue(element("forecast-level", in: app).appears(timeout: 15))
        sleep(3)
        keepScreenshots(light: "map-hours-3-wind", dark: "map-hours-dark-wind-closed")
        // The gusts, at the same hour.
        let gusts = element("what-gusts", in: app)
        XCTAssertTrue(gusts.appears(timeout: 15))
        gusts.tap()
        XCTAssertTrue(waitUntil(timeout: 30) { line.label.contains("13:00, forecast") && !line.label.hasPrefix("Wind at the ground") }, line.label)
        XCTAssertFalse(element("forecast-level", in: app).exists)
        sleep(3)
        keepScreenshot("map-hours-4-gusts")
        gusts.tap()
        usleep(700_000)

        // The cloud and the rain, at one o'clock.
        for (chip, name) in [("what-cloud", "5-cloud"), ("what-rain", "6-rain")] {
            showWhat(chip, in: app)
            XCTAssertTrue(chooseTime(endingWith: ", 13:00", in: app), "\(chip): \(chosenTime(in: app) ?? "")")
            XCTAssertTrue(waitUntil(timeout: 30) { line.label.contains("13:00, forecast") }, line.label)
            sleep(3)
            keepScreenshots(light: "map-hours-\(name)", dark: chip == "what-cloud" ? "map-hours-dark-cloud" : nil)
        }

        // The wind at a height, from the level's menu.
        showWhat("layer-wind", in: app)
        XCTAssertTrue(chooseTime(endingWith: ", 13:00", in: app))
        chooseForecastLevel("2000 m", in: app)
        XCTAssertTrue(waitUntil(timeout: 30) { line.label.hasPrefix("Wind at 2000 m, ") && line.label.contains("13:00, forecast") }, line.label)
        sleep(3)
        keepScreenshot("map-hours-7-wind-2000")

        // The whole key, the model's run and the notes, behind the "i".
        showWhatsKeyAndNotes(true, in: app)
        XCTAssertTrue(element("what-key-whole", in: app).appears(timeout: 15))
        XCTAssertTrue(element("what-notes", in: app).exists)
        usleep(400_000)
        keepScreenshots(light: "map-hours-7b-key-and-notes", dark: "map-hours-dark-wind")
        showWhatsKeyAndNotes(false, in: app)
        XCTAssertFalse(element("what-key-whole", in: app).exists)

        // At 1000 m: where the made-up ground lies higher there is no
        // wind, and a tapped square says so. Squares are tapped across
        // the map above the panel until one says it; each opens a sheet.
        chooseForecastLevel("1000 m", in: app)
        sleep(2)
        let sheet = element("forecast-hour-square", in: app)
        var none = false
        var kept = false
        search: for row in [0.3, 0.38, 0.22, 0.34, 0.26] {
            for column in [0.5, 0.3, 0.7, 0.15, 0.85] {
                app.coordinate(withNormalizedOffset: CGVector(dx: column, dy: row)).tap()
                guard sheet.appears(timeout: 5) else { continue }
                let title = element("forecast-hour-title", in: app).label
                XCTAssertTrue(element("forecast-square-days", in: app).appears(timeout: 10))
                if title.contains("No wind at 1000 m") {
                    if !none {
                        usleep(400_000)
                        keepScreenshot("map-hours-9-no-wind-at-1000")
                    }
                    none = true
                } else {
                    if !kept {
                        // A square with a wind: its arrow, and its days
                        // side by side.
                        usleep(400_000)
                        keepScreenshot("map-hours-8-square")
                    }
                    kept = true
                }
                app.buttons["Done"].tap()
                _ = sheet.vanishes(timeout: 5)
                if none, kept {
                    break search
                }
            }
        }

        // The wind at the ground again, as the next test expects it; then
        // what the squares had to say.
        chooseForecastLevel("Ground", in: app)
        for layer in others {
            setLayer(layer, on: true, in: app)
        }
        XCTAssertTrue(element("layer-wind", in: app).isSelected)
        XCTAssertTrue(kept, "No tap opened a square with a wind at 1000 m.")
        XCTAssertTrue(none, "No square said that there is no wind at 1000 m.")
    }

    /// The panel at a larger text size (the step above the largest of the
    /// standard sizes, about 135 %), against the stand-in with a made-up
    /// day: the chips, the line of time, the words with the height's menu
    /// and the gusts under them, and the "i" open, under the map's chips.
    /// Run on the narrowest simulator with `--first "iPhone 17e"`, this
    /// test being the first by name. What the screenshots show is for a
    /// person to look at; the test holds that the panel stays on the
    /// screen with everything on it to be had.
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        let panel = element("what-panel", in: app)
        let window = app.windows.firstMatch.frame

        // "Thermals": the line begins with the day as a whole.
        showWhat("what-thermals", in: app)
        XCTAssertTrue(timeCells(in: app).firstMatch.appears(timeout: 60))
        sleep(3)
        keepScreenshot("map-hours-larger-text-1-thermals")

        // The wind at a height: the words, and the menu under them.
        showWhat("layer-wind", in: app)
        XCTAssertTrue(chooseTime(endingWith: ", 13:00", in: app))
        chooseForecastLevel("2000 m", in: app)
        let line = element("what-line", in: app)
        XCTAssertTrue(waitUntil(timeout: 30) { line.label.hasPrefix("Wind at 2000 m, ") && line.label.contains("13:00") }, line.label)
        XCTAssertTrue(window.contains(element("forecast-level", in: app).frame))
        sleep(3)
        keepScreenshot("map-hours-larger-text-2-wind-2000")

        // The "i" open: the panel stays under the map's chips, and on the
        // screen.
        showWhatsKeyAndNotes(true, in: app)
        XCTAssertTrue(element("what-key-whole", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("map-hours-larger-text-3-key-and-notes")
        let chips = element("layer-wind", in: app).frame
        let open = panel.frame
        showWhatsKeyAndNotes(false, in: app)

        // As the next test expects it: the wind at the ground.
        chooseForecastLevel("Ground", in: app)
        XCTAssertGreaterThanOrEqual(open.minY, chips.maxY, "The open panel lies over the map's chips.")
        XCTAssertTrue(window.contains(open), "The open panel does not lie on the screen.")
    }

    /// Where the map opens for the radar's screenshots: far enough out for
    /// the whole of the development server's pictures (9.4 to 17.2 E,
    /// 46.3 to 48.5 N, the region's box, for both layers), their edge,
    /// and the analysis filling what the radars do not see, in the part
    /// of the map the panel leaves to be seen. The map that comes with the
    /// app ends long before: beyond it the pictures lie on the map's bare
    /// ground.
    private static let overTheRadar = "46.3,13.3,5"

    /// Whether a line is a moment's of both layers: the radars' time and
    /// age, and the time of the analysis under it. The times are on the
    /// simulator's clock, so they are looked at by their form.
    private func isAMomentOfBothLayers(_ notes: String, minutes: Int) -> Bool {
        let time = "\\d\\d:\\d\\d"
        let form = "Rain radar, \(time), \(minutes) minutes ago\\. Austria's analysis is of \(time)\\."
        return notes.range(of: form, options: .regularExpression) != nil
    }

    /// Whether the panel's line is the radar's: "Rain, 14:15, measured".
    private func isTheRadarsLine(_ line: String) -> Bool {
        line.range(of: "^Rain, \\d\\d:\\d\\d, measured$", options: .regularExpression) != nil
    }

    /// The rain on the map tab, against the development server, which has
    /// both radar sources recorded on 2026-11-21: DWD's radars with frames
    /// of 13:10 and 13:15 UTC, GeoSphere Austria's analysis with frames of
    /// 12:45 and 13:00, and a radar's clock that stands at 13:19. "Rain"
    /// chosen opens on the newest moment, both pictures on the map with
    /// the radars' on top and the analysis where the radars see nothing;
    /// behind the "i" the moment's ages, what each layer is, the key and
    /// the credits; a step to the moment before, and the moments played to
    /// the newest again. What the map shows is kept between launches: the
    /// test goes back to the wind at its end.
    func testTheMapShowsTheRainRadar() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", Self.overTheRadar])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        showWhat("layer-wind", in: app)
        XCTAssertFalse(element("what-rain", in: app).isSelected)

        showWhat("what-rain", in: app)
        // The panel: the radar's moments on the line, on the newest, and
        // its line in the core's words.
        let line = element("what-line", in: app)
        XCTAssertTrue(waitUntil(timeout: 60) { self.isTheRadarsLine(line.label) }, line.label)
        let newest = line.label
        XCTAssertTrue(element("radar-play", in: app).exists)
        XCTAssertFalse(element("what-key-whole", in: app).exists)
        XCTAssertTrue(element("what-key", in: app).exists)
        // Both sources' credits are behind the map's credits button while
        // their pictures are drawn.
        XCTAssertTrue(waitForCredit("Radar: Deutscher Wetterdienst and GeoSphere Austria", in: app, timeout: 30))
        // Time for the pictures to be drawn.
        sleep(4)
        keepScreenshot("map-radar-1-newest")

        // Behind the "i": the moment with its ages, what each layer is,
        // the key from the most rain down, what the radar did not see, the
        // note and both credits.
        showWhatsKeyAndNotes(true, in: app)
        let notes = element("what-notes", in: app)
        XCTAssertTrue(notes.appears(timeout: 15))
        XCTAssertTrue(isAMomentOfBothLayers(notes.label, minutes: 4), notes.label)
        XCTAssertTrue(notes.label.contains("Germany's radars, every 5 minutes"), notes.label)
        XCTAssertTrue(notes.label.contains("Austria's analysis of radar and gauges, every 15 minutes"), notes.label)
        XCTAssertTrue(notes.label.contains("Datenbasis: Deutscher Wetterdienst"), notes.label)
        XCTAssertTrue(notes.label.contains("Data source: GeoSphere Austria"), notes.label)
        XCTAssertTrue(element("what-key-whole", in: app).exists)
        XCTAssertTrue(labelled(containing: "Cloudburst", in: app).exists)
        XCTAssertTrue(labelled(containing: "Not seen by the radar", in: app).exists)
        usleep(400_000)
        keepScreenshot("map-radar-2-key-and-notes")
        showWhatsKeyAndNotes(false, in: app)

        // The moment before: a tap on the line's first cell. The same
        // analysis lies under the radars' picture before.
        let first = timeCells(in: app).firstMatch
        XCTAssertTrue(first.appears(timeout: 15))
        first.tap()
        XCTAssertTrue(waitUntil(timeout: 30) { self.isTheRadarsLine(line.label) && line.label != newest }, line.label)
        showWhatsKeyAndNotes(true, in: app)
        XCTAssertTrue(isAMomentOfBothLayers(element("what-notes", in: app).label, minutes: 9))
        showWhatsKeyAndNotes(false, in: app)
        sleep(2)
        keepScreenshot("map-radar-3-the-moment-before")

        // Played: through the moments, to stop at the newest.
        element("radar-play", in: app).tap()
        let played = waitUntil(timeout: 30) { line.label == newest }
        usleep(400_000)
        keepScreenshot("map-radar-4-played-to-the-newest")

        // On the line after the radar's moments: the forecast of rain.
        if chooseTime(endingWith: ", 13:00", in: app) {
            sleep(3)
            keepScreenshot("map-radar-5-forecast-after")
        }

        // Back to the wind, as the next test expects the map.
        showWhat("layer-wind", in: app)
        XCTAssertFalse(element("what-rain", in: app).isSelected)
        XCTAssertTrue(played, "The moments did not play to the newest.")
    }

    /// The rain's panel in the dark appearance, over the map, which stays
    /// light: closed and with the "i" open, the map closer in, over the
    /// south of Bavaria and Tirol, where a pixel of a picture is to be
    /// told from the next and the radars' picture gives way to the
    /// analysis south of the main ridge.
    func testTheRainRadarsCardInTheDark() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, dark: true, extra: ["--server", server, "--map-at", "47.2,11.6,7.4"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        showWhat("what-rain", in: app)
        let line = element("what-line", in: app)
        let shown = waitUntil(timeout: 60) { self.isTheRadarsLine(line.label) }
        sleep(4)
        keepScreenshot("map-radar-dark-1-card")
        showWhatsKeyAndNotes(true, in: app)
        let key = element("what-key-whole", in: app).appears(timeout: 15)
        usleep(400_000)
        keepScreenshot("map-radar-dark-2-key-and-notes")
        showWhatsKeyAndNotes(false, in: app)
        showWhat("layer-wind", in: app)
        XCTAssertTrue(shown, "The radar's line did not come.")
        XCTAssertTrue(key, "The radar's key did not open.")
    }

    /// The rain when Venti is not reached: the panel says so, with the one
    /// thing to do about it.
    func testTheRainRadarSaysWhenVentiIsNotReached() {
        let app = launch(bundledMap: true)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        showWhat("what-rain", in: app)
        let problem = element("radar-problem", in: app)
        let said = problem.appears(timeout: 60)
        let words = said ? problem.label : ""
        let again = app.buttons["Try again"].exists
        usleep(400_000)
        keepScreenshot("map-radar-not-reached")
        showWhat("layer-wind", in: app)
        XCTAssertTrue(words.hasPrefix("Venti could not be reached for the rain radar."), words)
        XCTAssertTrue(again)
    }

    /// The app's icon on the home screen: the design language's, which
    /// the screenshot is kept to show.
    func testTheAppHasItsIconOnTheHomeScreen() {
        let app = XCUIApplication()
        app.launchArguments = Self.naming(["--ui-testing"])
        app.launch()
        XCTAssertTrue(app.tabBars.firstMatch.appears(timeout: 60))
        XCUIDevice.shared.press(.home)

        // The icon is on whichever page of the home screen had room; on a
        // simulator whose home screen has it no more (the iPhone 17e's on
        // 2026-11-23), in the App Library after the last page.
        let home = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let icon = home.descendants(matching: .any)
            .matching(NSPredicate(format: "label == 'Venti' AND elementType IN %@", [
                XCUIElement.ElementType.icon.rawValue, XCUIElement.ElementType.button.rawValue,
            ]))
            .firstMatch
        _ = icon.appears(timeout: 5)
        for _ in 0..<5 where !(icon.exists && icon.isHittable) {
            home.swipeLeft()
            sleep(1)
        }
        XCTAssertTrue(icon.exists && icon.isHittable, "Venti's icon is on no page of the home screen.")
        usleep(400_000)
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

        XCTAssertTrue(app.staticTexts["The forecast could not be read"].appears(timeout: 90))
        XCTAssertTrue(app.buttons["Try again"].exists)
        XCTAssertTrue(element("today-headline", in: app).exists)
        usleep(400_000)
        keepScreenshot("today-0-not-connected")
        app.buttons["Settings"].firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 15))
        XCTAssertFalse(app.textFields["Server address"].exists)
        // The section "Venti", further down: the account and the maps.
        let accountRow = element("account-link", in: app)
        for _ in 0..<4 where !accountRow.exists {
            app.swipeUp()
        }
        XCTAssertTrue(accountRow.appears(timeout: 10))
        XCTAssertFalse(app.textFields["Server address"].exists)
        usleep(400_000)
        keepScreenshot("settings-0-venti")
        // The tabs stay while the settings are open: the Fly tab is the
        // cockpit.
        openTheCockpit(in: app)
        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))
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
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 30))
        // A flight covers the tab bar; back to the logbook first.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(element("logbook-row", in: app).appears(timeout: 15))

        openTab("Assistant", in: app)
        let question = app.buttons["How was my flying this year?"]
        if !question.appears(timeout: 8) {
            // The first tap can still be swallowed by the bar's animation.
            openTab("Assistant", in: app)
        }
        XCTAssertTrue(question.appears(timeout: 15))
        keepScreenshot("assistant-1-start")
        question.tap()

        XCTAssertTrue(labelled(containing: "Your logbook has one flight", in: app).appears(timeout: 20))
        XCTAssertTrue(labelled(containing: "Looked up your list of flights", in: app).exists)
        usleep(400_000)
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
        let app = launch(
            bundledMap: true, extra: ["--server", server, "--map-at", "47.6642,11.7967,11"]
        )
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        for layer in quiet {
            setLayer(layer, on: false, in: app)
        }
        setLayer("layer-airspace", on: false, in: app)
        setLayer("layer-pilots", on: true, in: app)

        // The count of the Pilots layer, in the core's words, and the
        // network's credit with the map's credits.
        let pilots = layersRow("Pilots in the air", in: app)
        XCTAssertTrue(pilots.appears(timeout: 60))
        let counted = NSPredicate(format: "value CONTAINS 'others in the air'")
        expectation(for: counted, evaluatedWith: pilots)
        waitForExpectations(timeout: 30)
        closeLayers(in: app)
        XCTAssertTrue(waitForCredit("Open Glider Network", in: app, timeout: 15))
        sleep(3)
        keepScreenshots(light: "others-1-map", dark: "others-3-map-dark")

        // The paraglider at the map's middle, which lies where the
        // map's insets put it, some 11 points above the window's. Not
        // the hang glider: the label of Venti's landed demo glider
        // lies beside it, and Venti's pilots are on top for a tap.
        let middle = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        let line = element("other-line", in: app)
        for nudge in [0.0, -8, 8] where !line.exists {
            middle.withOffset(CGVector(dx: 0, dy: -11 + nudge)).tap()
            _ = line.appears(timeout: 4)
        }
        XCTAssertTrue(line.appears(timeout: 5))
        XCTAssertEqual(line.label, "Paraglider at 2,140 m, climbing 1.8 m/s, 20 s ago")
        XCTAssertEqual(element("other-name", in: app).label, "WB, Paraglider")
        XCTAssertTrue(element("other-credit", in: app).exists)
        XCTAssertTrue(app.navigationBars["Paraglider"].exists)
        usleep(400_000)
        keepScreenshots(light: "others-2-sheet", dark: "others-4-sheet-dark")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["map-layers"].appears(timeout: 15))
        for layer in quiet {
            setLayer(layer, on: true, in: app)
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
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        setLayer("layer-pilots", on: true, in: app)
        let pilots = layersRow("Pilots in the air", in: app, timeout: 30)
        XCTAssertTrue(pilots.exists)
        pilots.tap()
        XCTAssertTrue(app.navigationBars["In the air"].appears(timeout: 15))
        // The glider in the air, not the one that landed.
        let flying = app.buttons
            .matching(NSPredicate(format: "identifier == 'live-row' AND NOT (label CONTAINS 'Landed')"))
            .firstMatch
        XCTAssertTrue(flying.appears(timeout: 15))
        flying.tap()

        let follow = app.buttons["live-keep-on-map"]
        XCTAssertTrue(follow.appears(timeout: 20))
        XCTAssertEqual(follow.label, "Follow on the map")
        XCTAssertFalse(follow.isSelected)
        // The page moves the map to the flight first.
        sleep(3)
        follow.tap()
        XCTAssertTrue(follow.isSelected)
        sleep(3)
        keepScreenshots(light: "live-follow-1-on", dark: "live-follow-dark-1-on")

        // A drag of the map above the page lets it go.
        let from = app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.22))
        from.press(forDuration: 0.1, thenDragTo: from.withOffset(CGVector(dx: 90, dy: 40)))
        let off = NSPredicate(format: "isSelected == false")
        expectation(for: off, evaluatedWith: follow)
        waitForExpectations(timeout: 10)
        usleep(400_000)
        keepScreenshots(light: "live-follow-2-let-go", dark: "live-follow-dark-2-let-go")

        // On again, and the page closed: off when it opens again.
        follow.tap()
        XCTAssertTrue(follow.isSelected)
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["map-layers"].appears(timeout: 15))
        layersRow("Pilots in the air", in: app, timeout: 30).tap()
        XCTAssertTrue(flying.appears(timeout: 15))
        flying.tap()
        XCTAssertTrue(follow.appears(timeout: 20))
        XCTAssertFalse(follow.isSelected)
        app.buttons["Done"].tap()
    }

    /// Two stations named "Weissfluhjoch" above Davos, MeteoSwiss's and
    /// the SLF's: the list tells them apart by the network at the end of
    /// each row's line, and a station's page names its network under its
    /// name.
    func testTheListTellsTwoStationsOfOneNameApartByTheirNetwork() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "46.83,9.81,11"])
        XCTAssertTrue(app.buttons["map-layers"].appears(timeout: 90))
        XCTAssertTrue(waitForCredit("Wind", in: app))
        layersRow("Wind stations", in: app).tap()
        XCTAssertTrue(app.navigationBars["Wind stations"].appears(timeout: 15))
        let rows = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'station-row' AND label == 'Weissfluhjoch'"))
        XCTAssertTrue(rows.firstMatch.appears(timeout: 30))
        for _ in 0..<8 where !(isOnScreen(rows.element(boundBy: 0), in: app) && rows.count == 2
            && isOnScreen(rows.element(boundBy: 1), in: app))
        {
            app.swipeUp()
            usleep(500_000)
        }
        XCTAssertEqual(rows.count, 2)
        let lines = (0..<rows.count).map { (rows.element(boundBy: $0).value as? String) ?? "" }
        XCTAssertTrue(lines.contains { $0.hasSuffix("MeteoSwiss") }, "\(lines)")
        XCTAssertTrue(lines.contains { $0.hasSuffix("SLF") }, "\(lines)")
        usleep(400_000)
        keepScreenshot("wind-networks-1-list")

        // The SLF's page names its network under the name.
        let slf = (0..<rows.count).map { rows.element(boundBy: $0) }
            .first { (($0.value as? String) ?? "").hasSuffix("SLF") }
        try XCTUnwrap(slf).tap()
        let bar = app.navigationBars["Weissfluhjoch"]
        XCTAssertTrue(bar.appears(timeout: 15))
        XCTAssertTrue(bar.staticTexts["SLF"].appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("wind-networks-2-station")
    }

    /// Live wind, from a Venti server running on the same machine in its
    /// development mode: the stations of nine networks as they were on
    /// one evening (28 in the first region). The CI
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

        XCTAssertTrue(app.buttons["map-layers"].appears(timeout: 90))
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
        usleep(400_000)
        keepScreenshot("wind-1a-layers")
        closeLayers(in: app)
        // The credits in full, one tap from the map.
        app.buttons["map-credits"].tap()
        XCTAssertTrue(element("wind-credits", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("wind-1b-credits")
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)

        layersRow("Wind stations", in: app).tap()
        XCTAssertTrue(app.navigationBars["Wind stations"].appears(timeout: 15))
        XCTAssertTrue(element("station-row", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("wind-2-stations")
        // The list is by name and longer than the screen: the stations
        // further down are brought in.
        let byName = { (name: String) -> XCUIElement in
            let row = self.labelled(containing: name, in: app)
            for _ in 0..<8 where !self.isOnScreen(row, in: app) {
                app.swipeUp()
                usleep(500_000)
            }
            return row
        }
        // A name the provider writes in capitals is shown in ordinary case.
        XCTAssertTrue(byName("Patscherkofel").exists)
        let summit = byName("Zugspitze")
        XCTAssertTrue(summit.exists)

        summit.tap()
        XCTAssertTrue(element("wind-chart", in: app).appears(timeout: 30))
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
        XCTAssertTrue(picker.appears(timeout: 10))
        picker.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        let dayBefore = app.buttons
            .matching(NSPredicate(format: "label CONTAINS '17' AND label CONTAINS 'November'"))
            .firstMatch
        XCTAssertTrue(dayBefore.appears(timeout: 10))
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
        XCTAssertTrue(pastDay.appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Day", in: app).exists)
        sleep(2)
        keepScreenshot("station-2-past-day")

        // A station whose source gives no height and no temperature: a
        // pilots' Pioupiou sensor on the Kampenwand. Its page says neither
        // "0 m" nor a unit after a dash.
        app.navigationBars.buttons.firstMatch.tap()
        let sensor = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'station-row' AND label CONTAINS 'Kampenwand'"))
            .firstMatch
        for _ in 0..<8 where !isOnScreen(sensor, in: app) {
            app.swipeDown()
            usleep(500_000)
        }
        if sensor.exists {
            sensor.tap()
            // The development server has one reading of it: no chart,
            // the page's line that it has no history yet.
            let page = app.descendants(matching: .any)
                .matching(NSPredicate(
                    format: "identifier IN %@ OR label BEGINSWITH 'The server has no history'",
                    ["wind-chart", "station-day-empty"]
                ))
                .firstMatch
            XCTAssertTrue(page.appears(timeout: 30))
            XCTAssertFalse(element("station-height", in: app).exists)
            XCTAssertFalse(app.descendants(matching: .any)["Height"].exists)
            let temperature = app.descendants(matching: .any)["Temperature"].firstMatch
            if temperature.exists {
                XCTAssertEqual(temperature.value as? String, "–")
            }
            sleep(2)
            keepScreenshot("station-3-no-height")
            app.navigationBars.buttons.firstMatch.tap()
        } else {
            XCTFail("No Pioupiou station on the Kampenwand in the list")
        }

        // Back to the map.
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["map-layers"].appears(timeout: 15))

        // After the stations, because a glider's page moves the map to its
        // flight, and the list of stations is of the part of the map in view.
        // Who is in the air, from the same server: its development mode
        // has two gliders over the Tegernsee, one flying and one landed.
        let pilots = layersRow("Pilots in the air", in: app, timeout: 30)
        XCTAssertTrue(pilots.exists)
        pilots.tap()
        XCTAssertTrue(app.navigationBars["In the air"].appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Demo Pilot", in: app).appears(timeout: 15))
        XCTAssertTrue(saying("Landed", in: app).exists)
        usleep(400_000)
        keepScreenshot("wind-1b-in-the-air")
        // Choosing one closes the list and opens the glider's page over
        // the map, which draws the flight so far and moves to show it.
        element("live-row", in: app).tap()
        XCTAssertTrue(element("live-age", in: app).appears(timeout: 20))
        sleep(5)
        keepScreenshot("wind-1c-glider")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["map-layers"].appears(timeout: 15))

        // The assistant looks up the same wind on the same server.
        openTab("Assistant", in: app)
        let question = app.buttons["How is the wind on the Zugspitze right now?"]
        XCTAssertTrue(question.appears(timeout: 15))
        question.tap()
        XCTAssertTrue(labelled(containing: "Zugspitze: 54 km/h", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "the wind stations around a place", in: app).exists)
        usleep(400_000)
        keepScreenshot("wind-4-assistant")

        // What the pilots of the server share with everyone: on the
        // League tab. The server's development mode has public flights
        // of several made-up pilots.
        openTab("Community", in: app)
        let everyone = app.buttons["Everyone"]
        XCTAssertTrue(everyone.appears(timeout: 15))
        everyone.tap()
        let flight = element("public-flight-row", in: app)
        XCTAssertTrue(flight.appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Mara Muster", in: app).exists)
        usleep(400_000)
        keepScreenshot("community-1-everyone")
        // The pilots of the server by their best flights. The development
        // mode's flights are of 2025 and 2026; every year together holds
        // them whatever year the test runs in.
        app.buttons["Ranking"].tap()
        let allYears = app.buttons["All years"]
        XCTAssertTrue(allYears.appears(timeout: 30))
        allYears.tap()
        XCTAssertTrue(element("ranked-pilot", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Mara Muster", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("community-1b-ranking")
        // The rule stands under the pilots: with the development server's
        // Lena Probe among them it is below the screen's end.
        let rule = labelled(containing: "best 3 flights shared with everyone", in: app)
        for _ in 0..<4 where !rule.exists {
            app.swipeUp()
        }
        XCTAssertTrue(rule.appears(timeout: 5))
        for _ in 0..<4 where !everyone.isHittable {
            app.swipeDown()
        }
        everyone.tap()
        XCTAssertTrue(flight.appears(timeout: 30))
        // Another pilot's flight opens like one's own.
        flight.tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 40))
        sleep(6)
        keepScreenshot("community-2-flight")
        // From the flight to everything its pilot shares.
        app.buttons["More"].tap()
        let ofPilot = labelled(containing: "Flights of", in: app)
        XCTAssertTrue(ofPilot.appears(timeout: 10))
        ofPilot.tap()
        // The pilot's totals stand over the flights: the page has come
        // once a total is there, and the flights are below the screen's
        // end, where a list has not built them yet.
        XCTAssertTrue(element("pilot-total", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("community-3-pilot")
        let ofTheirs = element("pilot-flight-row", in: app)
        for _ in 0..<6 where !ofTheirs.exists {
            app.swipeUp()
        }
        XCTAssertTrue(ofTheirs.appears(timeout: 10))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 20))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(everyone.appears(timeout: 15))
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        // What the pilot was told: a row of the account, the newest line
        // on top.
        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        bring("account-notifications", into: app).tap()
        XCTAssertTrue(app.navigationBars["Notifications"].appears(timeout: 15))
        XCTAssertTrue(element("notification", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Lena Probe (@lena) follows you", in: app).exists)
        let commented = labelled(containing: "Mara Muster (@mara) commented on your flight", in: app)
        XCTAssertTrue(commented.exists)
        XCTAssertTrue(labelled(containing: "Lena Probe (@lena) cheered your flight", in: app).exists)
        usleep(400_000)
        keepScreenshot("social-1-notifications")

        // A line about a flight opens that flight: the pilot's own, so
        // without a cheer button.
        commented.tap()
        let comments = element("flight-comments", in: app)
        XCTAssertTrue(comments.appears(timeout: 40))
        XCTAssertEqual(comments.label, "2 comments")
        XCTAssertTrue(labelled(containing: "2 pilots cheered", in: app).appears(timeout: 15))
        XCTAssertFalse(element("flight-cheer", in: app).exists)
        sleep(6)
        keepScreenshot("social-2-own-flight")
        comments.tap()
        let count = element("comments-count", in: app)
        XCTAssertTrue(count.appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Well flown. How was the air above the ridge?", in: app).exists)
        XCTAssertTrue(labelled(containing: "Mara Muster · ", in: app).exists)
        // Under one's own flight every comment can be removed, and
        // another pilot's reported.
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "comment-remove").count, 2)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "comment-report").count, 1)
        XCTAssertTrue(element("comment-text", in: app).exists)
        usleep(400_000)
        keepScreenshot("social-3-comments")
        element("comments-close", in: app).tap()
        XCTAssertTrue(comments.appears(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Notifications"].appears(timeout: 15))

        // The feed: the flights of the pilots followed, each with what
        // pilots made of it, as cards on Community and whole a tap away.
        openTab("Community", in: app)
        XCTAssertTrue(element("feed-card", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("social-4-community")
        let following = bring("community-following-all", into: app)
        XCTAssertTrue(following.appears(timeout: 15))
        following.tap()
        let rows = app.descendants(matching: .any).matching(identifier: "feed-row")
        XCTAssertTrue(rows.firstMatch.appears(timeout: 30))
        let maras = rows.matching(
            NSPredicate(format: "label CONTAINS 'Mara Muster' AND label CONTAINS 'You and 1 other cheered'")
        ).firstMatch
        for _ in 0..<4 where !maras.exists {
            app.swipeUp()
        }
        XCTAssertTrue(maras.appears(timeout: 10))
        XCTAssertTrue(maras.label.contains("1 comment"), maras.label)
        usleep(400_000)
        keepScreenshot("social-4-feed")

        // Another pilot's flight: cheered, and written under.
        let lenas = rows.matching(NSPredicate(format: "label CONTAINS 'Lena Probe'")).firstMatch
        for _ in 0..<4 where !isOnScreen(lenas, in: app) {
            app.swipeUp()
        }
        XCTAssertTrue(lenas.appears(timeout: 10))
        // Clear of the tab bar, which floats over the list's foot.
        bringUp(lenas, in: app)
        lenas.tap()
        let cheer = element("flight-cheer", in: app)
        XCTAssertTrue(cheer.appears(timeout: 40))
        if cheer.label != "Cheered" {
            cheer.tap()
        }
        XCTAssertTrue(
            app.buttons.matching(NSPredicate(format: "identifier == 'flight-cheer' AND label == 'Cheered'"))
                .firstMatch.appears(timeout: 20)
        )
        // The pilot alone cheered: the button says it, and no line beside
        // it says it again.
        XCTAssertFalse(element("flight-cheers-line", in: app).exists)
        XCTAssertTrue(comments.appears(timeout: 15))
        comments.tap()
        let text = element("comment-text", in: app)
        XCTAssertTrue(text.appears(timeout: 15))
        text.tap()
        text.typeText("Nice line along the ridge.")
        XCTAssertTrue(saying("26 of 500", in: app).appears(timeout: 10))
        XCTAssertTrue(element("comment-notice", in: app).exists)
        element("comment-send", in: app).tap()
        // Its age goes by Venti's clock as read, advanced by the time
        // since: the development server's clock stands still, so the
        // comment it dates "now" is a minute or two old by then.
        let written = app.descendants(matching: .any).matching(NSPredicate(
            format: "label CONTAINS 'Demo Pilot · ' AND (label CONTAINS 'just now' OR label CONTAINS 'min ago')"
        )).firstMatch
        XCTAssertTrue(written.appears(timeout: 20))
        XCTAssertTrue(element("comment-remove", in: app).exists)
        XCTAssertFalse(element("comment-report", in: app).exists)
        usleep(400_000)
        keepScreenshot("social-5-written")
        element("comments-close", in: app).tap()
        XCTAssertTrue(comments.appears(timeout: 15))
        XCTAssertTrue(comments.label.contains("comment"), comments.label)
        sleep(4)
        keepScreenshot("social-6-cheered")

        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(rows.firstMatch.appears(timeout: 15))
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(following.appears(timeout: 15))

        // A pilot's page, from the field over Community: the counts, and
        // the button that follows them.
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.appears(timeout: 15))
        field.tap()
        field.typeText("timo")
        let timo = labelled(containing: "Timo Entwurf (@timo)", in: app)
        XCTAssertTrue(timo.appears(timeout: 30))
        timo.tap()
        let follow = element("pilot-follow", in: app)
        XCTAssertTrue(follow.appears(timeout: 30))
        let follows = element("pilot-follows", in: app)
        XCTAssertTrue(follows.label.contains("Follows"), follows.label)
        usleep(400_000)
        keepScreenshot("social-7-pilot")
        if follow.label == "Follow" {
            follow.tap()
        }
        XCTAssertTrue(
            app.buttons.matching(NSPredicate(format: "identifier == 'pilot-follow' AND label == 'Following'"))
                .firstMatch.appears(timeout: 20)
        )
        XCTAssertTrue(follows.label.contains("1 follower"), follows.label)
        usleep(400_000)
        keepScreenshot("social-8-following")

        // The feed in the dark. A second start is signed out.
        let dark = launch(bundledMap: true, dark: true, extra: ["--server", server])
        XCTAssertTrue(dark.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: dark)
        openTab("Community", in: dark)
        XCTAssertTrue(element("feed-card", in: dark).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("social-9-community-dark")
        bring("community-following-all", into: dark).tap()
        XCTAssertTrue(element("feed-row", in: dark).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("social-10-feed-dark")
    }

    /// A long press on the map shows the server's forecast for the place
    /// under the finger. Skipped without a server.
    func testALongPressOnTheMapShowsTheForecastThere() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        sleep(3)
        // The map opens on the northern Alps; the left of its upper half
        // lies inside the server's first region.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.35, dy: 0.4)).press(forDuration: 1.3)
        XCTAssertTrue(element("forecast-table", in: app).appears(timeout: 40))
        XCTAssertTrue(labelled(containing: "hPa", in: app).exists)
        usleep(400_000)
        keepScreenshot("map-forecast")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 10))
    }

    /// A task: the cockpit's settings say which one the cockpit will fly,
    /// and its page shows it. In a replay of the sample flight held just
    /// before it leaves the start cylinder, the cockpit counts down to the
    /// start, on the map's page and on the task page with the next
    /// turnpoint's name and readings; run on, it takes the start and leads
    /// on to the next turnpoint, and is held again. (The task as a QR code
    /// is behind a menu of the cockpit's settings, which XCTest cannot tap
    /// over the cockpit in less than a minute a tap: no UI test shows it.)
    func testATaskLeadsThroughTheCockpit() {
        let app = launch(bundledMap: true, extra: ["--sample-task"] + Self.replay(from: 360, holdingAfter: 0))
        openTheCockpitsSettings(in: app)
        let row = element("task-before-takeoff", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        XCTAssertTrue(saying("4 turnpoints", in: app).exists)
        row.tap()
        XCTAssertTrue(app.navigationBars["Task"].appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Sample turnpoint west", in: app).appears(timeout: 10))
        XCTAssertTrue(element("task-sketch", in: app).exists)
        usleep(400_000)
        keepScreenshot("task-1-task")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 15))

        // Held half a minute before the flight leaves the start cylinder.
        replayTheSampleFlight(in: app)
        XCTAssertTrue(labelled(containing: "Replay paused", in: app).appears(timeout: 10))
        // Where the flight stands in its task is on the map's page.
        nextPage(in: app)
        let strip = element("task-strip", in: app)
        XCTAssertTrue(strip.appears(timeout: 30))
        XCTAssertTrue(valued(containing: "Sample start", in: app).appears(timeout: 10))
        keepScreenshot("task-3-before-start")
        // The task's own page, after the flight's: the next turnpoint.
        nextPage(in: app)
        nextPage(in: app)
        XCTAssertTrue(element("task-next", in: app).appears(timeout: 15))
        XCTAssertTrue(element("task-next-name", in: app).exists)
        XCTAssertTrue(element("task-next-over", in: app).exists)
        usleep(400_000)
        keepScreenshot("cockpit-task-1-page")

        // Around to the map's page, and run on: out of the start cylinder,
        // on task, and held again.
        nextPage(in: app)
        nextPage(in: app)
        XCTAssertTrue(strip.appears(timeout: 10))
        setReplay(running: true, in: app)
        XCTAssertTrue(valued(containing: "On task", in: app).appears(timeout: 30))
        setReplay(running: false, in: app)
        XCTAssertTrue(valued(containing: "Sample turnpoint west", in: app).appears(timeout: 10))
        app.buttons["Show the whole flight"].tap()
        XCTAssertTrue(app.buttons["Follow the glider"].appears(timeout: 10))
        sleep(2)
        keepScreenshot("task-4-on-task")

        // The whole task with what is reached, and the two buttons for
        // when the task and the flight have come apart.
        strip.tap()
        XCTAssertTrue(app.navigationBars["Task"].appears(timeout: 10))
        XCTAssertTrue(element("task-turnpoint", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("task-5-progress")
        // The buttons are below the turnpoints, further down the sheet.
        app.swipeUp()
        XCTAssertTrue(app.buttons["Skip to the next turnpoint"].appears(timeout: 10))
        app.buttons["Done"].tap()

        // The task page on task: the next turnpoint, and no count down.
        nextPage(in: app)
        nextPage(in: app)
        XCTAssertTrue(element("task-next", in: app).appears(timeout: 15))
        XCTAssertFalse(element("task-start-words", in: app).exists)
        usleep(400_000)
        keepScreenshot("cockpit-task-2-on-task")

        endTheCockpit(in: app)
        XCTAssertTrue(app.buttons["Open the cockpit"].appears(timeout: 15))
    }

    /// Cables by the Tegernsee, which the development server has recorded:
    /// the Wallberg's gondola drawn on the map, and a tap on it gives its
    /// words, its credit and the core's sentence on what is not mapped.
    func testTheMapShowsCablesByTheTegernsee() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(
            bundledMap: true, extra: ["--server", server, "--map-at", "47.6696709,11.7834441,14.5"]
        )
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        setLayer("layer-cables", on: true, in: app)
        XCTAssertTrue(waitForCredit("Cables: OpenStreetMap contributors", in: app))
        sleep(2)
        keepScreenshots(light: "map-cables-1-drawn", dark: "map-cables-dark-1-drawn")
        // The line passes through the middle of the map.
        let words = element("cable-words", in: app)
        for _ in 0..<5 where !words.exists {
            app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
            _ = words.appears(timeout: 3)
        }
        XCTAssertTrue(words.exists)
        XCTAssertTrue(words.label.contains("Gondola"), words.label)
        // The card has the cable's words and the credit, and what the
        // layer cannot promise is behind its "i" in the layers sheet.
        XCTAssertTrue(element("cable-credits", in: app).exists)
        XCTAssertFalse(element("cable-limits", in: app).exists)
        usleep(400_000)
        keepScreenshots(light: "map-cables-2-tapped", dark: "map-cables-dark-2-tapped")
        app.buttons["Done"].tap()
        usleep(700_000)
        XCTAssertTrue(openLayers(in: app))
        let more = element("cable-limits-more", in: app)
        for _ in 0..<2 where !isOnScreen(more, in: app) {
            app.swipeUp()
            usleep(700_000)
        }
        more.tap()
        XCTAssertTrue(element("cable-limits", in: app).appears(timeout: 5))
        usleep(400_000)
        keepScreenshots(light: "map-cables-3-limits", dark: "map-cables-dark-3-limits")
        closeLayers(in: app)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        // The map has the server's answer once the sites' credit is there:
        // the Wallberg's ramps are in view. No wind station is.
        XCTAssertTrue(waitForCredit("Sites", in: app))
        // Both start off, whatever a test before this one left.
        setLayer("layer-climbs", on: false, in: app)
        setLayer("layer-flown", on: false, in: app)

        XCTAssertTrue(openLayers(in: app))
        let climbs = layersSwitch("layer-climbs", in: app)
        XCTAssertTrue(climbs.appears(timeout: 10))
        XCTAssertTrue(layersSwitch("layer-flown", in: app).exists)
        XCTAssertTrue(labelled(containing: "Climbs now", in: app).exists)
        XCTAssertTrue(labelled(containing: "Where thermals have been", in: app).exists)
        usleep(400_000)
        keepScreenshot("air-1-layers-off")
        set(climbs, on: true)
        // The layer says how many climb in the part of the map in view,
        // in the core's words: "1 climb in the last 10 minutes".
        let counted = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS %@ OR label CONTAINS %@", "climb in the last", "climbs in the last")
        ).firstMatch
        XCTAssertTrue(counted.appears(timeout: 30))
        usleep(400_000)
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
            _ = line.appears(timeout: 3)
        }
        XCTAssertTrue(line.exists)
        XCTAssertTrue(line.label.hasPrefix("Climbing now: +2.4 m/s"), line.label)
        XCTAssertTrue(labelled(containing: "Open Glider Network", in: app).exists)
        usleep(400_000)
        keepScreenshot("air-4-climb-line")
        element("air-line-close", in: app).tap()
        usleep(800_000)

        // Where thermals have been: its cells on the map, and a cell's
        // line where the tap finds the one up and left of the middle.
        setLayer("layer-climbs", on: false, in: app)
        XCTAssertTrue(openLayers(in: app))
        let flown = layersSwitch("layer-flown", in: app)
        XCTAssertTrue(flown.appears(timeout: 10))
        set(flown, on: true)
        closeLayers(in: app)
        sleep(4)
        keepScreenshot("air-5-map-flown")
        let cell = element("flown-line", in: app)
        middle.withOffset(CGVector(dx: -165, dy: -122)).tap()
        if cell.appears(timeout: 5) {
            usleep(400_000)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        XCTAssertTrue(waitForCredit("Sites", in: app))
        layersRow("Flying sites", in: app).tap()
        let ramp = app.descendants(matching: .any).matching(identifier: "site-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Take-off", "47.665"))
            .firstMatch
        XCTAssertTrue(ramp.appears(timeout: 30))
        bringUp(ramp, in: app)
        ramp.tap()
        let line = element("site-air", in: app)
        XCTAssertTrue(line.appears(timeout: 30))
        XCTAssertTrue(line.label.contains("in the air here"), line.label)
        usleep(400_000)
        keepScreenshot("air-7-site-pilots-in-the-air")
    }

    /// The text size above which the larger-text tests look: the largest
    /// of the standard ones, as the forecast card's test has it.
    private static let largerText = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL"]

    /// Whether the run asks for the screens with larger text too
    /// (`ui-tests.sh --larger`, which also runs the tests named
    /// `…LargerText`): the steps of other tests that launch again with
    /// larger text are left out without it.
    private static let largerTextAsked = ProcessInfo.processInfo.environment["VENTI_LARGER_TEXT"] == "1"

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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)
        XCTAssertTrue(waitForCredit("Sites", in: app))
        setLayer("layer-flown", on: false, in: app)

        XCTAssertTrue(openLayers(in: app))
        let climbs = layersSwitch("layer-climbs", in: app)
        XCTAssertTrue(climbs.appears(timeout: 10))
        set(climbs, on: true)
        sleep(2)
        keepScreenshot("larger-dark-1-layers")
        app.swipeUp()
        usleep(400_000)
        keepScreenshot("larger-dark-1b-layers-lower")
        closeLayers(in: app)
        sleep(2)
        keepScreenshot("larger-dark-2-map-climbs")

        app.buttons["map-credits"].tap()
        XCTAssertTrue(app.navigationBars["Credits"].appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("larger-dark-3-credits")
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)
        setLayer("layer-climbs", on: false, in: app)

        layersRow("Flying sites", in: app).tap()
        let ramp = app.descendants(matching: .any).matching(identifier: "site-row")
            .matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Take-off", "47.665"))
            .firstMatch
        XCTAssertTrue(ramp.appears(timeout: 30))
        bringUp(ramp, in: app)
        ramp.tap()
        XCTAssertTrue(element("site-air", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("larger-dark-4-site-pilots-in-the-air")
        let fold = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "How this is read")).firstMatch
        for _ in 0..<10 where !isOnScreen(fold, in: app) {
            drawUp(in: app)
        }
        XCTAssertTrue(fold.exists)
        usleep(400_000)
        keepScreenshot("larger-dark-5-site-fold-row")

        bring("alert-me", into: app, down: true).tap()
        XCTAssertTrue(element("alert-kind", in: app).appears(timeout: 15))
        usleep(400_000)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        bring("account-notifications", into: app).tap()
        XCTAssertTrue(app.navigationBars["Notifications"].appears(timeout: 15))
        XCTAssertTrue(element("notification", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("larger-dark-social-1-notifications")

        labelled(containing: "commented on your flight", in: app).tap()
        let comments = element("flight-comments", in: app)
        XCTAssertTrue(comments.appears(timeout: 40))
        sleep(6)
        keepScreenshot("larger-dark-social-2-flight-pills")
        comments.tap()
        XCTAssertTrue(element("comments-count", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("larger-dark-social-3-comments")
        element("comments-close", in: app).tap()
        XCTAssertTrue(comments.appears(timeout: 15))
        // A flight's page covers the tab bar: back to the notifications.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Notifications"].appears(timeout: 15))

        openTab("Community", in: app)
        XCTAssertTrue(element("feed-card", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("larger-dark-social-4-community")
        bring("community-following-all", into: app).tap()
        XCTAssertTrue(element("feed-row", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("larger-dark-social-5-feed")
        app.navigationBars.buttons.firstMatch.tap()

        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.appears(timeout: 15))
        field.tap()
        field.typeText("timo")
        let timo = labelled(containing: "Timo Entwurf (@timo)", in: app)
        XCTAssertTrue(timo.appears(timeout: 30))
        timo.tap()
        XCTAssertTrue(element("pilot-follow", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("larger-dark-social-6-pilot")
    }

    /// The pilot's picture on the account's page: initials first, a
    /// picture sent ("--sample-picture" stands in for the photo picker,
    /// which a test cannot reach), the switch for being among the pilots
    /// near, the picture removed again; then Community's pilots near the
    /// map's middle by the Tegernsee, where the development mode's Mara
    /// has a picture, her page with it, and its report. Light and dark,
    /// then the account and the pilots near with larger text.
    func testThePilotsPictureAndThePilotsNear() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        // Where the iPhone is, should it be allowed to say: the same place as the
        // map's middle, so that "pilots near" is of one place either way.
        XCUIDevice.shared.location = XCUILocation(location: CLLocation(latitude: 47.66, longitude: 11.8))
        let pilot = try await makeAccount(on: server, named: "Picture Tester", handlePrefix: "face")
        let place = ["--server", server, "--sample-picture", "--map-at", "47.66,11.8,11"]
        var app = launch(bundledMap: true, extra: place)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(pilot, in: app)
        openAccount(in: app)

        // Initials while there is no picture.
        let face = element("account-picture", in: app)
        XCTAssertTrue(face.appears(timeout: 30))
        XCTAssertEqual(face.value as? String, "Initials")
        usleep(400_000)
        keepScreenshots(light: "picture-1-initials", dark: "picture-dark-1-initials")

        // A picture sent: Venti's answer names it, and it is drawn.
        element("account-picture-change", in: app).tap()
        let pictured = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'account-picture' AND value == 'Picture'")).firstMatch
        XCTAssertTrue(pictured.appears(timeout: 30))
        XCTAssertTrue(element("account-picture-remove", in: app).exists)
        sleep(2)
        keepScreenshots(light: "picture-2-picture", dark: "picture-dark-2-picture")

        // Among the pilots near a place: on until switched off.
        let near = bring("account-shown-near", into: app)
        XCTAssertTrue(near.appears(timeout: 10))
        XCTAssertEqual(near.value as? String, "1")
        usleep(400_000)
        keepScreenshots(light: "picture-3-shown-near", dark: "picture-dark-3-shown-near")

        // Removed again: the initials.
        bring("account-picture-remove", into: app, down: true).tap()
        let initials = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'account-picture' AND value == 'Initials'")).firstMatch
        XCTAssertTrue(initials.appears(timeout: 30))

        // Community: the pilots near the map's middle, Mara with her
        // picture among them.
        openTab("Community", in: app)
        let mara = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'near-pilot' AND label CONTAINS 'Mara Muster'")).firstMatch
        for _ in 0..<6 where !isOnScreen(mara, in: app) {
            if mara.exists { app.swipeUp() } else { usleep(500_000) }
        }
        XCTAssertTrue(mara.appears(timeout: 30))
        XCTAssertTrue(element("near-words", in: app).label.contains("Within"), element("near-words", in: app).label)
        sleep(2)
        keepScreenshots(light: "picture-4-pilots-near", dark: "picture-dark-4-pilots-near")

        // Her page has her picture, and a way to report it.
        mara.tap()
        let picture = element("pilot-picture", in: app)
        XCTAssertTrue(picture.appears(timeout: 30))
        XCTAssertEqual(picture.value as? String, "Picture")
        sleep(2)
        keepScreenshots(light: "picture-5-pilot-page", dark: "picture-dark-5-pilot-page")
        app.buttons["More"].tap()
        let report = app.buttons["Report picture"]
        XCTAssertTrue(report.appears(timeout: 10))
        report.tap()
        XCTAssertTrue(element("report-reason", in: app).appears(timeout: 10))
        XCTAssertTrue(app.navigationBars["Report picture"].exists)
        usleep(400_000)
        keepScreenshots(light: "picture-6-report-picture", dark: "picture-dark-6-report-picture")
        app.buttons["Cancel"].tap()

        // With the text size above the largest standard one, launched
        // anew: the account with its picture, and the pilots near.
        guard Self.largerTextAsked else { return }
        app.terminate()
        app = launch(bundledMap: true, extra: place + Self.largerText)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(pilot, in: app)
        openAccount(in: app)
        XCTAssertTrue(element("account-picture", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("picture-larger-1-account")
        openTab("Community", in: app)
        let maraAgain = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'near-pilot' AND label CONTAINS 'Mara Muster'")).firstMatch
        XCTAssertTrue(maraAgain.appears(timeout: 30))
        sleep(2)
        keepScreenshot("picture-larger-4-pilots-near")
    }

    /// Community as the demo pilot: a plan posted from it is a card with
    /// its day, its place, the pilot's face and "I am in", which steps
    /// out and in again; the flights of the pilots followed as cards; and
    /// the newest of what the pilot was told. Light and dark, then the
    /// cards with larger text.
    func testCommunityGathersPlansFeedAndNotifications() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        var app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)
        openTab("Community", in: app)

        // A plan for today, from the bar's "New plan".
        let add = element("community-new-plan", in: app)
        XCTAssertTrue(add.appears(timeout: 30))
        add.tap()
        let place = element("plan-place", in: app)
        XCTAssertTrue(place.appears(timeout: 10))
        place.tap()
        let name = "Wallberg \(Int.random(in: 100...999))"
        place.typeText(name)
        app.buttons["Save"].tap()
        let card = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'plan-card' AND label CONTAINS %@", name)).firstMatch
        XCTAssertTrue(card.appears(timeout: 30))
        XCTAssertTrue(card.label.contains("Today"), card.label)
        sleep(2)
        keepScreenshots(light: "community-hub-1-plans", dark: "community-hub-dark-1-plans")

        // Its author is in from the start; "I am in" steps out and in.
        let isIn = app.buttons
            .matching(NSPredicate(format: "identifier == 'plan-card-in' AND value == %@", name)).firstMatch
        XCTAssertTrue(isIn.appears(timeout: 10))
        // The cards stand side by side: the new one is brought in, the
        // row first up or down into the screen (with the larger text the
        // cards are taller than what is left of it), then by swiping a
        // card that is on it.
        let window = app.windows.firstMatch.frame
        for _ in 0..<8 where !isOnScreen(isIn, in: app) {
            if isIn.frame.minY < window.minY + 100 {
                app.swipeDown()
            } else if isIn.frame.maxY > window.maxY - 100 {
                app.swipeUp()
            } else {
                let cards = app.descendants(matching: .any).matching(identifier: "plan-card")
                let shown = cards.allElementsBoundByIndex.first { isOnScreen($0, in: app) }
                (shown ?? card).swipeLeft()
            }
            usleep(500_000)
        }
        XCTAssertTrue(isIn.isSelected)
        isIn.tap()
        for _ in 0..<40 where isIn.isSelected {
            usleep(250_000)
        }
        XCTAssertFalse(isIn.isSelected)
        isIn.tap()
        for _ in 0..<40 where !isIn.isSelected {
            usleep(250_000)
        }
        XCTAssertTrue(isIn.isSelected)

        // The feed's cards and the newest lines told, further down.
        let feedCard = bring("feed-card", into: app)
        XCTAssertTrue(feedCard.appears(timeout: 30))
        usleep(400_000)
        keepScreenshots(light: "community-hub-2-feed", dark: "community-hub-dark-2-feed")
        let told = bring("community-notification", into: app)
        XCTAssertTrue(told.appears(timeout: 30))
        usleep(400_000)
        keepScreenshots(light: "community-hub-3-pilots-near-and-told", dark: "community-hub-dark-3-pilots-near-and-told")

        // The card opens the plan.
        app.swipeDown()
        app.swipeDown()
        app.swipeDown()
        XCTAssertTrue(card.appears(timeout: 10))
        card.tap()
        XCTAssertTrue(element("plan-in", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshots(light: "community-hub-4-plan", dark: "community-hub-dark-4-plan")

        // With the text size above the largest standard one, launched
        // anew: the plans' cards and the feed.
        guard Self.largerTextAsked else { return }
        app.terminate()
        app = launch(bundledMap: true, extra: ["--server", server] + Self.largerText)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)
        openTab("Community", in: app)
        XCTAssertTrue(element("plan-card", in: app).appears(timeout: 30))
        sleep(2)
        keepScreenshot("community-hub-larger-1-plans")
        _ = bring("feed-card", into: app)
        usleep(400_000)
        keepScreenshot("community-hub-larger-2-feed")
    }

    /// An open plan for everyone, posted from Community as the demo pilot
    /// at a place named at random, where the map is: first refused for a
    /// place without a name in the core's words, then a card among the
    /// open plans near, its page with what only its author may do, and
    /// deleted again. Light and dark. The development server's own open
    /// plans are for 19 and 20 November and so past by the iPhone's
    /// clock; the core's tests and the server's hold who may do what.
    func testAnOpenPlanIsPostedSeenAndDeleted() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)
        openTab("Community", in: app)

        let add = bring("community-new-open-plan", into: app)
        XCTAssertTrue(add.appears(timeout: 30))
        add.tap()
        let place = element("open-plan-place", in: app)
        XCTAssertTrue(place.appears(timeout: 10))
        // Without a name the core refuses, in its own words.
        let post = element("open-plan-save", in: app)
        post.tap()
        let refused = element("open-plan-problem", in: app)
        XCTAssertTrue(refused.appears(timeout: 10))
        XCTAssertTrue(refused.label.contains("Say where"), refused.label)

        place.tap()
        let name = "Kampen \(Int.random(in: 100...999))"
        place.typeText(name)
        XCTAssertTrue(refused.vanishes(timeout: 5))
        app.buttons["Looking for company"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Looking for company"].firstMatch.isSelected)
        XCTAssertTrue(element("open-plan-position", in: app).exists)
        usleep(400_000)
        keepScreenshots(light: "open-plans-1-form", dark: "open-plans-dark-1-form")
        post.tap()

        // The card, by the place's name, with the core's offer.
        let card = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier == 'open-plan-card' AND label == %@", name)).firstMatch
        XCTAssertTrue(card.appears(timeout: 30))
        XCTAssertTrue(card.value.map { "\($0)" }?.contains("Looking for company") ?? false)
        let isIn = app.buttons
            .matching(NSPredicate(format: "identifier == 'open-plan-card-in' AND value == %@", name)).firstMatch
        XCTAssertTrue(isIn.appears(timeout: 10))
        // Its author is in from the start.
        XCTAssertTrue(isIn.isSelected)
        for _ in 0..<8 where !isOnScreen(card, in: app) {
            card.frame.minY < app.windows.firstMatch.frame.midY ? app.swipeDown() : app.swipeUp()
            usleep(400_000)
        }
        usleep(400_000)
        keepScreenshots(light: "open-plans-2-cards", dark: "open-plans-dark-2-cards")

        // Its page: the author changes and deletes it.
        card.tap()
        XCTAssertTrue(element("open-plan-change", in: app).appears(timeout: 10))
        XCTAssertTrue(element("open-plan-public", in: app).exists)
        XCTAssertFalse(element("open-plan-report", in: app).exists)
        usleep(400_000)
        keepScreenshots(light: "open-plans-3-page", dark: "open-plans-dark-3-page")
        let delete = bring("open-plan-delete", into: app)
        delete.tap()
        let confirm = element("open-plan-delete-confirm", in: app)
        XCTAssertTrue(confirm.appears(timeout: 10))
        confirm.tap()
        XCTAssertTrue(card.vanishes(timeout: 20))
    }

    /// A route of the pilot's own waypoints: tapped in the order to fly
    /// them and kept as the task.
    func testARouteIsPlannedFromWaypoints() {
        let app = launch(bundledMap: true, extra: ["--sample-waypoints"])
        openTheCockpitsSettings(in: app)
        let row = element("task-before-takeoff", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        row.tap()
        let plan = app.buttons["Plan a route from your waypoints"]
        XCTAssertTrue(plan.appears(timeout: 15))
        plan.tap()
        XCTAssertTrue(app.navigationBars["Plan a route"].appears(timeout: 10))
        XCTAssertFalse(app.buttons["Keep"].isEnabled)

        labelled(containing: "Sample landing field", in: app).tap()
        labelled(containing: "Sample turnpoint", in: app).tap()
        XCTAssertTrue(app.buttons["Keep"].isEnabled)
        usleep(400_000)
        keepScreenshot("route-1-plan")

        app.buttons["Keep"].tap()
        XCTAssertTrue(labelled(containing: "its waypoints", in: app).appears(timeout: 10))
        XCTAssertTrue(element("task-sketch", in: app).exists)
        usleep(400_000)
        keepScreenshot("route-2-kept")

        // Without waypoints: a route drawn on the map, a tap for each
        // place. Three places and back to the first make a triangle, and
        // the page says what it would score.
        let draw = element("task-draw-route", in: app)
        for _ in 0..<4 where !(draw.exists && draw.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(draw.appears(timeout: 10))
        draw.tap()
        let map = element("route-map", in: app)
        XCTAssertTrue(map.appears(timeout: 20))
        XCTAssertTrue(labelled(containing: "Tap the map where the route begins", in: app).appears(timeout: 10))
        XCTAssertFalse(app.buttons["Keep"].isEnabled)
        // Time for the map to draw before it is tapped.
        sleep(4)
        for (x, y) in [(0.3, 0.45), (0.7, 0.45), (0.5, 0.2)] {
            map.coordinate(withNormalizedOffset: CGVector(dx: x, dy: y)).tap()
            // A single tap waits for the map's double tap to be ruled out.
            sleep(1)
        }
        XCTAssertTrue(labelled(containing: "along the route", in: app).appears(timeout: 10))
        app.buttons["Back to the start"].tap()
        XCTAssertTrue(labelled(containing: "triangle", in: app).appears(timeout: 10))
        sleep(2)
        keepScreenshot("route-3-drawn")

        app.buttons["Keep"].tap()
        XCTAssertTrue(labelled(containing: "Turnpoint 1", in: app).appears(timeout: 15))
        usleep(400_000)
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
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.68,11.75,11"])
        drawATriangle(in: app)
        let toggle = element("route-weather-toggle", in: app)
        XCTAssertTrue(toggle.appears(timeout: 10))
        XCTAssertEqual(toggle.value as? String, "Hidden")
        toggle.tap()
        XCTAssertTrue(element("route-weather-words", in: app).appears(timeout: 30))
        // The forecast's model and credits once Venti has answered.
        _ = element("route-weather-credits", in: app).appears(timeout: 30)
        let departure = element("route-weather-departure", in: app)
        XCTAssertTrue(departure.appears(timeout: 30))
        // The heights follow the forecast, once the ground is read.
        for _ in 0..<30 where !departure.label.contains(" m ") {
            sleep(1)
        }
        XCTAssertTrue(departure.label.contains(" m "), departure.label)
        XCTAssertGreaterThanOrEqual(app.descendants(matching: .any).matching(identifier: "route-weather-point").count, 3)
        sleep(2)
        keepScreenshots(light: "route-weather-1", dark: "route-weather-3-dark")
        // Further down the card: the places, the legs and the credits.
        element("route-weather-words", in: app).swipeUp()
        usleep(400_000)
        keepScreenshot("route-weather-2-places")
    }

    /// What a glide reaches from the Wallberg: a long press on the map,
    /// "What a glide reaches from here" in what opens, and the area on the
    /// map with the core's words on the card under it, light and dark.
    func testAGlideReachesFromAPlacePressedOnTheMap() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.6642,11.7967,12"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        // The development server's demo glider stands in the map's
        // middle, and the stations' arrows near it: off while pressing.
        setLayer("layer-pilots", on: false, in: app)
        setLayer("layer-wind", on: false, in: app)
        sleep(3)
        element("overview-map", in: app).coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 1.3)
        let entry = element("glide-reach-here", in: app)
        XCTAssertTrue(entry.appears(timeout: 30))
        entry.tap()
        let words = element("glide-reach-words", in: app)
        XCTAssertTrue(words.appears(timeout: 60))
        XCTAssertTrue(words.label.hasPrefix("From"), words.label)
        XCTAssertTrue(element("glide-reach-note", in: app).exists)
        // A step of the glide asks the core again, and the card says so.
        let before = words.label
        let named = app.buttons["glide-reach-glide-Increment"]
        (named.exists ? named : element("glide-reach-glide", in: app).buttons["Increment"]).tap()
        for _ in 0..<20 where words.label == before {
            usleep(250_000)
        }
        XCTAssertNotEqual(words.label, before)
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
        keepScreenshots(light: "glide-reach-1", dark: "glide-reach-2-dark")
        element("glide-reach-clear", in: app).tap()
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 10))
        setLayer("layer-pilots", on: true, in: app)
        setLayer("layer-wind", on: true, in: app)
    }

    /// Opens the map to draw a route on from the Fly tab, and draws three
    /// places on it.
    private func drawATriangle(in app: XCUIApplication) {
        openTheCockpitsSettings(in: app)
        let row = element("task-before-takeoff", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        row.tap()
        let draw = element("task-draw-route", in: app)
        for _ in 0..<4 where !(draw.exists && draw.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(draw.appears(timeout: 10))
        draw.tap()
        let map = element("route-map", in: app)
        XCTAssertTrue(map.appears(timeout: 20))
        // Time for the map to draw before it is tapped.
        sleep(4)
        for (x, y) in [(0.3, 0.35), (0.7, 0.35), (0.5, 0.15)] {
            map.coordinate(withNormalizedOffset: CGVector(dx: x, dy: y)).tap()
            // A single tap waits for the map's double tap to be ruled out.
            sleep(1)
        }
        XCTAssertTrue(labelled(containing: "along the route", in: app).appears(timeout: 10))
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
        // The same server knows the flying sites. They are drawn once the
        // map shows a valley and not a country: the map opens on the
        // Tegernsee's. (The pilot's flights move the map only while "My
        // flights" is switched on, which it is not until the pilot does.)
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.67,11.68,9.3"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        // The map tab's airspace is off from here to the end: what the
        // cockpit warns from must not depend on what that map draws.
        setAirspace(on: false, in: app)
        XCTAssertTrue(waitForCredit("Sites", in: app))
        sleep(4)
        keepScreenshot("sites-1-map")

        // The signs on the map, as a list, and one of them.
        layersRow("Flying sites", in: app).tap()
        XCTAssertTrue(app.navigationBars["Flying sites"].appears(timeout: 15))
        let row = element("site-row", in: app)
        XCTAssertTrue(row.appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("sites-2-list")
        row.tap()
        // The page is known by the button at its head: what is further
        // down depends on how much the forecast has to show of the day.
        XCTAssertTrue(app.buttons["Plan route"].appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("sites-3-site")

        // A route that begins here: the map to draw it on opens with the
        // site as its first place.
        app.buttons["Plan route"].tap()
        XCTAssertTrue(element("route-map", in: app).appears(timeout: 20))
        XCTAssertTrue(app.navigationBars["Draw a route"].exists)
        // The route can be drawn on photographs from above as well.
        XCTAssertTrue(app.buttons["map-aerial"].exists)
        sleep(3)
        keepScreenshot("sites-3a-plan-route")
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.buttons["Plan route"].appears(timeout: 15))

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
        XCTAssertTrue(element("forecast-table", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "hPa", in: app).exists)
        usleep(400_000)
        keepScreenshot("sites-4-forecast")
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(forecast.appears(timeout: 10))

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
        if !done.appears(timeout: 5) {
            app.navigationBars.buttons.firstMatch.tap()
        }
        XCTAssertTrue(done.appears(timeout: 10))
        done.tap()

        // Nothing has asked for airspace so far: the map tab's is off, and
        // the app does not know where the pilot is. (The Fly tab is the
        // cockpit, which asks for the position: it is not opened here
        // before the pilot has allowed it on Today.)
        let line = element("airspace-before-takeoff", in: app)

        // The pilot is by the Tegernsee and lets Venti know, on Today.
        put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
        openTab("Today", in: app)
        // The button is at the page's foot, under the take-offs, and a row
        // that is not on screen does not exist yet.
        XCTAssertTrue(element("today-card", in: app).appears(timeout: 60))
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
        XCTAssertTrue(allow.appears(timeout: 15))
        allow.tap()
        for _ in 0..<3 {
            put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
            sleep(1)
        }
        keepScreenshot("airspace-0a-today-with-the-position")

        // The Fly tab asks for the airspace around the pilot by itself, and
        // the cockpit around the glider. A location manager started after
        // the simulated position was set hears of it only once it changes:
        // the iPhone wanders by half a metre each second, as a receiver's
        // positions do, until the cockpit has a fix.
        openTheCockpit(in: app)
        let noFix = labelled(containing: "No GPS", in: app)
        for _ in 0..<20 where noFix.exists {
            put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
            sleep(1)
        }
        keepScreenshot("airspace-0b-cockpit-with-the-position")
        XCTAssertFalse(noFix.exists, "The cockpit has no position: \(noFix.label)")
        // The cockpit's settings say what it will warn from.
        openTheCockpitsSettings(in: app)
        XCTAssertTrue(line.appears(timeout: 30))
        let openAIP = saying("openAIP", in: app)
        for _ in 0..<30 where !openAIP.exists {
            put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
            sleep(1)
        }
        XCTAssertTrue(openAIP.exists)
        keepScreenshot("airspace-1-before-takeoff")

        line.tap()
        XCTAssertTrue(element("airspace-from-server", in: app).appears(timeout: 15))
        XCTAssertTrue(saying("not certified", in: app).exists)
        keepScreenshot("airspace-2-from-server")
        app.navigationBars.buttons.firstMatch.tap()

        // The cockpit, waiting for the take-off, has that airspace to warn
        // from, and the map names whose it is.
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 15))
        app.navigationBars["Settings"].buttons["Done"].tap()
        XCTAssertTrue(element("cockpit-end", in: app).appears(timeout: 30))
        XCTAssertTrue(element("cockpit-airspace-credit", in: app).appears(timeout: 30))
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
        // The development server's airspace lies farther than the reading's
        // five kilometres from the take-off: the map page has no reading,
        // and without one no strip (as on Android), the map its room.
        nextPage(in: app)
        XCTAssertTrue(element("target-strip", in: app).appears(timeout: 30))
        put(metresNorth: 0, altitude: Self.takeoffGroundM, speed: 0)
        usleep(400_000)
        XCTAssertFalse(element("airspace-proximity-map", in: app).exists)
        keepScreenshot("airspace-3-cockpit-before-takeoff")
        endTheCockpit(in: app)

        // And the map tab's airspace was off all the while.
        openTab("Map", in: app)
        let button = app.buttons["map-airspace"]
        XCTAssertTrue(button.appears(timeout: 30))
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))

        openTab("Settings", in: app)
        // The row is below the first screenful, and a row that is not on
        // screen does not exist yet.
        let link = element("account-link", in: app)
        for _ in 0..<4 where !(link.exists && link.isHittable) {
            app.swipeUp()
        }
        if !link.appears(timeout: 10) {
            keepScreenshot("account-0-settings-without-the-link")
        }
        XCTAssertTrue(link.exists)
        link.tap()
        let handle = openThePasswordForm(in: app)
        XCTAssertTrue(handle.appears(timeout: 15))
        keepScreenshot("account-1-sign-in")
        handle.tap()
        handle.typeText("demo")
        let password = app.secureTextFields["Password"]
        password.tap()
        password.typeText("demo-pilot-password")
        element("account-submit", in: app).tap()

        // Signed in, and the sync that follows has fetched the flights.
        XCTAssertTrue(labelled(containing: "@demo", in: app).appears(timeout: 30))
        // The iPhone offers to keep the password. Not in a test.
        for asking in [app, XCUIApplication(bundleIdentifier: "com.apple.springboard")] {
            let notNow = asking.buttons["Not Now"]
            if notNow.appears(timeout: 3) {
                notNow.tap()
                break
            }
        }
        XCTAssertTrue(app.buttons["Sync now"].appears(timeout: 60))
        sleep(2)
        keepScreenshot("account-2-signed-in")

        openTab("Logbook", in: app)
        let row = element("logbook-row", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("account-3-logbook")

        // Share the newest flight with those who have its link. Whatever
        // the demo pilot had chosen for it before, that is the choice now.
        row.tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 30))
        app.buttons["More"].tap()
        app.buttons["Share with a link"].tap()
        let withTheLink = element("share-unlisted", in: app)
        XCTAssertTrue(withTheLink.appears(timeout: 15))
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
        XCTAssertTrue(chosen.appears(timeout: 20))
        XCTAssertTrue(labelled(containing: "/f/", in: app).exists)
        usleep(400_000)
        keepScreenshot("account-4-share")
        if let before {
            element("share-\(before)", in: app).tap()
            XCTAssertTrue(chosenChoice(before).appears(timeout: 20))
        }

        // Something another pilot shares that should not be there is
        // reported to whoever runs the server: from the flight's menu, with
        // a reason, from the account signed in above. The server's
        // development mode takes the report and keeps it nowhere.
        app.buttons["Done"].tap()
        app.navigationBars.buttons.firstMatch.tap()
        openTab("Community", in: app)
        let everyone = app.buttons["Everyone"]
        XCTAssertTrue(everyone.appears(timeout: 15))
        everyone.tap()
        let theirs = element("public-flight-row", in: app)
        XCTAssertTrue(theirs.appears(timeout: 30))
        theirs.tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 40))
        app.buttons["More"].tap()
        let report = app.buttons["Report this flight"]
        XCTAssertTrue(report.appears(timeout: 10))
        report.tap()
        let reason = app.buttons.matching(NSPredicate(format: "label CONTAINS 'Not a real flight'")).firstMatch
        XCTAssertTrue(reason.appears(timeout: 10))
        // Nothing can be sent before a reason is chosen.
        XCTAssertFalse(app.buttons["Send"].isEnabled)
        reason.tap()
        usleep(400_000)
        keepScreenshot("community-4-report")
        app.buttons["Send"].tap()
        XCTAssertTrue(element("report-outcome", in: app).appears(timeout: 20))
        XCTAssertTrue(labelled(containing: "Whoever runs Venti will read it", in: app).exists)
        usleep(400_000)
        keepScreenshot("community-5-reported")
        app.buttons["Done"].tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 15))

        // Who flies where: the plans of the pilot's groups, from the map.
        // The development mode's demo pilot keeps a made-up group.
        app.navigationBars.buttons.firstMatch.tap()
        openTab("Map", in: app)
        let plans = layersRow("Flying plans", in: app, timeout: 15)
        XCTAssertTrue(plans.exists)
        plans.tap()
        XCTAssertTrue(element("plans-groups", in: app).appears(timeout: 30))

        // A plan for today, posted to the group. Its place is taken from
        // the pilot's own waypoints, which know where they are, so the
        // plan can be marked on the map.
        app.buttons["New plan"].tap()
        XCTAssertTrue(element("plan-place", in: app).appears(timeout: 10))
        choose("Sample landing field", from: app.buttons["From my waypoints"], in: app)
        XCTAssertTrue(labelled(containing: "On the map", in: app).appears(timeout: 10))
        app.buttons["Save"].tap()
        let posted = labelled(containing: "Sample landing field", in: app)
        XCTAssertTrue(posted.appears(timeout: 20))
        XCTAssertTrue(app.staticTexts["Today"].exists || labelled(containing: "Today", in: app).exists)
        usleep(400_000)
        keepScreenshot("plans-1-board")

        // The plan itself: its author is in from the start, and can step
        // out again.
        posted.tap()
        XCTAssertTrue(element("plan-in", in: app).appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "I am out", in: app).exists)
        usleep(400_000)
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
        if theirPlan.appears(timeout: 5) {
            theirPlan.tap()
            let reportPlan = element("plan-report", in: app)
            XCTAssertTrue(reportPlan.appears(timeout: 10))
            reportPlan.tap()
            XCTAssertTrue(element("report-reason", in: app).appears(timeout: 10))
            XCTAssertTrue(app.navigationBars["Report this plan"].exists)
            usleep(400_000)
            keepScreenshot("groups-plan-report")
            app.buttons["Cancel"].tap()
            XCTAssertTrue(reportPlan.appears(timeout: 10))
            app.navigationBars.buttons.firstMatch.tap()
        }
        // One's own plan has no report entry.
        posted.tap()
        XCTAssertTrue(element("plan-in", in: app).appears(timeout: 10))
        XCTAssertFalse(element("plan-report", in: app).exists)
        app.navigationBars.buttons.firstMatch.tap()

        // And a group the pilot is in and does not keep: the development
        // mode's other group, joined with its code. The keeper of a group
        // deletes it instead.
        let groupsLink = element("plans-groups", in: app)
        XCTAssertTrue(groupsLink.appears(timeout: 10))
        groupsLink.tap()
        let join = app.buttons["Join with a code"]
        XCTAssertTrue(join.appears(timeout: 10))
        join.tap()
        let code = app.alerts.textFields.firstMatch
        XCTAssertTrue(code.appears(timeout: 10))
        code.typeText("5555-5555-5555-5555")
        app.alerts.buttons["Join"].tap()
        let otherGroup = labelled(containing: "Beispielgruppe", in: app)
        XCTAssertTrue(otherGroup.appears(timeout: 20))
        otherGroup.tap()
        let reportGroup = element("group-report", in: app)
        for _ in 0..<3 where !reportGroup.exists {
            app.swipeUp()
        }
        XCTAssertTrue(reportGroup.appears(timeout: 15))
        reportGroup.tap()
        XCTAssertTrue(element("report-reason", in: app).appears(timeout: 10))
        XCTAssertTrue(app.navigationBars["Report this group"].exists)
        usleep(400_000)
        keepScreenshot("groups-group-report")
        app.buttons["Cancel"].tap()
        XCTAssertTrue(reportGroup.appears(timeout: 10))
        // Back to the board.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(join.appears(timeout: 10))
        app.navigationBars.buttons.firstMatch.tap()

        // The map marks the plan where it is, and says so.
        XCTAssertTrue(element("plans-groups", in: app).appears(timeout: 10))
        app.buttons["Done"].tap()
        let map = element("overview-map", in: app)
        XCTAssertTrue(map.appears(timeout: 15))
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
        XCTAssertTrue(toGroups.appears(timeout: 10))
        toGroups.tap()
        let group = element("group-row", in: app)
        XCTAssertTrue(group.appears(timeout: 15))
        group.tap()
        XCTAssertTrue(element("group-member", in: app).appears(timeout: 20))
        XCTAssertTrue(app.buttons["New invite code"].exists)
        usleep(400_000)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        openTab("Community", in: app)
        let everyone = app.buttons["Everyone"]
        XCTAssertTrue(everyone.appears(timeout: 15))
        everyone.tap()
        // The demo pilot's one public flight, by its pilot: not by its
        // place in the list, which grows with the development mode.
        let rows = app.descendants(matching: .any).matching(identifier: "public-flight-row")
        let demo = rows.matching(NSPredicate(format: "label CONTAINS %@", "Demo Pilot")).firstMatch
        XCTAssertTrue(demo.appears(timeout: 40))
        demo.tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 40))

        // From the flight's details: lay another flight beside it.
        app.buttons["Flight details"].tap()
        let compare = element("flight-compare", in: app)
        XCTAssertTrue(compare.appears(timeout: 10))
        compare.tap()
        // What other pilots flew there that day, and the pilot's own.
        let other = element("compare-public-row", in: app)
        XCTAssertTrue(other.appears(timeout: 40))
        XCTAssertTrue(labelled(containing: "Your logbook holds no other flight", in: app).exists)
        usleep(400_000)
        keepScreenshot("compare-1-choose")
        other.tap()

        // Both flights on the page: the second track on the map, its
        // altitude in the profile, and a line that says whose it is.
        let beside = element("flight-beside", in: app)
        XCTAssertTrue(beside.appears(timeout: 40))
        sleep(3)
        keepScreenshot("compare-2-beside")

        // The two in numbers.
        beside.tap()
        XCTAssertTrue(app.navigationBars["Side by side"].appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "Airtime", in: app).appears(timeout: 10))
        XCTAssertTrue(labelled(containing: "Highest", in: app).exists)
        usleep(400_000)
        keepScreenshot("compare-3-numbers")

        // Taken away again, the page is the one flight's as before. The
        // button is under the numbers, below what a half-high sheet shows.
        let end = element("compare-end", in: app)
        for _ in 0..<4 where !(end.exists && end.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(end.appears(timeout: 10))
        end.tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 10))
        XCTAssertFalse(element("flight-beside", in: app).exists)
    }

    /// The ranking of a month, a pilot found by name, and the pilot's
    /// page with what their public flights add up to. Runs against a
    /// Venti server in development mode, whose made-up pilot Lena Probe
    /// shares three flights with everyone, of May, June and July 2026.
    func testAPilotIsFoundRankedByMonthAndAddedUp() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        openTab("Community", in: app)

        // The ranking of 2026, and under the years its months that have
        // one, after the year itself. May is far right in the row.
        let ranking = app.buttons["Ranking"]
        XCTAssertTrue(ranking.appears(timeout: 15))
        ranking.tap()
        let year = app.buttons["2026"].firstMatch
        XCTAssertTrue(year.appears(timeout: 30))
        year.tap()
        let may = app.buttons["May"].firstMatch
        XCTAssertTrue(may.appears(timeout: 30))
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
        XCTAssertTrue(labelled(containing: "May 2026", in: app).appears(timeout: 20))
        XCTAssertTrue(labelled(containing: "Place 1, Lena Probe", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshots(light: "pilots-1-ranking-of-a-month", dark: "pilots-dark-1-ranking-of-a-month")

        // A pilot is found by two letters of the name, in the field over
        // Community.
        app.navigationBars.buttons["Community"].tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.appears(timeout: 15))
        field.tap()
        field.typeText("le")
        let found = labelled(containing: "Lena Probe (@lena), 3 public flights", in: app)
        XCTAssertTrue(found.appears(timeout: 30))
        usleep(400_000)
        keepScreenshots(light: "pilots-2-pilot-found", dark: "pilots-dark-2-pilot-found")

        // The line opens the pilot's page: the totals over the flights.
        found.tap()
        let totals = app.descendants(matching: .any).matching(identifier: "pilot-total")
        let points = totals.matching(NSPredicate(format: "label CONTAINS %@", "Points, best 3 flights")).firstMatch
        XCTAssertTrue(points.appears(timeout: 30))
        XCTAssertEqual(points.value as? String, "28.19")
        // Under the pilot's face and name: a list builds a row once it
        // is scrolled to.
        let site = labelled(containing: "Musterkogel West, 2 flights", in: app)
        let months = element("flown-months", in: app)
        for _ in 0..<4 where !(site.exists && months.exists) {
            drawUp(in: app)
        }
        XCTAssertTrue(site.appears(timeout: 15))
        XCTAssertTrue(months.exists)
        usleep(400_000)
        keepScreenshots(light: "pilots-3-pilot-totals", dark: "pilots-dark-3-pilot-totals")

        // A take-off flown from opens the site's page. This step only
        // adds a screenshot.
        if isOnScreen(site, in: app) {
            site.tap()
            if element("site-tier", in: app).appears(timeout: 20) {
                usleep(400_000)
                keepScreenshots(light: "pilots-4-pilot-site", dark: "pilots-dark-4-pilot-site")
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
        let app = launch(bundledMap: true, extra: ["--server", server] + Self.replay(from: 100, holdingAfter: 600))
        openTheCockpitsSettings(in: app)

        // Before take-off the cockpit's settings say whether there is a map for
        // flying without a network, and leads to the regions.
        let before = element("map-before-takeoff", in: app)
        XCTAssertTrue(before.appears(timeout: 30))
        before.tap()
        // On a runner whose simulator has only just started, the app can
        // be slow to answer for half a minute, and a tap made then is
        // sometimes lost: the page is asked for once more.
        if !app.navigationBars["Offline maps"].appears(timeout: 20) {
            before.tap()
        }
        XCTAssertTrue(app.navigationBars["Offline maps"].appears(timeout: 30))
        let region = element("map-region", in: app)
        if !region.appears(timeout: 30) {
            if element("maps-none", in: app).exists {
                throw XCTSkip("The server hands out no maps: it was started without --maps.")
            }
            return XCTFail("The server's maps did not appear")
        }
        XCTAssertTrue(labelled(containing: "Tegernsee valley", in: app).exists)
        usleep(400_000)
        keepScreenshot("maps-1-offered")

        // The pilot is told the size before anything is fetched.
        element("map-download", in: app).tap()
        let go = app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", "Download 7")).firstMatch
        XCTAssertTrue(go.appears(timeout: 10))
        go.tap()
        // Seven megabytes from the same machine: fetched, checked against
        // what the server said, and kept.
        XCTAssertTrue(element("map-delete", in: app).appears(timeout: 90))
        XCTAssertTrue(labelled(containing: "On this device", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("maps-2-kept")

        // Back in the settings, the region is named among what to check.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars["Settings"].appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Tegernsee valley", in: app).appears(timeout: 10))

        // The sample flight is over the Tegernsee, inside the region: the
        // cockpit draws its map from the download, and its top line says
        // nothing of a missing map.
        replayTheSampleFlight(in: app)
        XCTAssertTrue(element("Altitude", in: app).appears(timeout: 20))
        XCTAssertFalse(element("cockpit-check-map", in: app).exists)
        // Time for the map to be made anew and to draw.
        sleep(6)
        keepScreenshot("maps-3-cockpit")
        // The height over the ground is read from the region's elevation.
        XCTAssertTrue(saying("over ground", in: app).appears(timeout: 30))

        // The wind on the ground, from the server's wind stations: the
        // nearest in a badge, and all of them in the wind's sheet, which
        // begins with them when it is opened from that badge. The
        // development mode's stations stand fifty kilometres off and
        // more, and the badge says so.
        let ground = element("ground-wind-badge", in: app)
        XCTAssertTrue(ground.appears(timeout: 120))
        tapWhenStill(ground)
        XCTAssertTrue(element("ground-wind-row", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("maps-4-wind-on-the-ground")
        app.buttons["Done"].tap()
    }

    /// The same screens with tiles fetched from the public servers, as a
    /// pilot's phone draws them. Only the screenshots matter here: the test
    /// does not fail when the network is slow.
    func testTheMapWithTilesFromTheNetwork() {
        // "My flights" on the map tab for this launch alone, so that the
        // map tab shows the valley of the sample flight further down.
        let app = launch(bundledMap: false, extra: ["-VentiMapShowsMyFlights", "YES"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        sleep(12)
        keepScreenshot("online-1-map")

        openTab("Logbook", in: app)
        app.buttons["Add a sample flight"].tap()
        XCTAssertTrue(app.buttons["Flight details"].appears(timeout: 30))
        sleep(12)
        keepScreenshot("online-2-flight")

        // The aerial view: photographs from above under the flight, at
        // the round layers button. It is one choice for every map, so the
        // map tab shows it too, until the button there switches it back.
        let aerial = app.buttons["map-aerial"]
        XCTAssertTrue(aerial.appears(timeout: 15))
        // The choice is kept between launches: an earlier run that
        // stopped half-way may have left it on.
        if aerial.isSelected {
            aerial.tap()
        }
        XCTAssertFalse(aerial.isSelected)
        aerial.tap()
        XCTAssertTrue(aerial.isSelected)
        // Whose photographs they are is said on the map while they show.
        XCTAssertTrue(element("photo-credits", in: app).appears(timeout: 10))
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
        XCTAssertTrue(photos.appears(timeout: 30))
        sleep(12)
        XCTAssertTrue(
            photos.label.contains("Photos: ") && photos.label.contains("Bayerische Vermessungsverwaltung · EOX"),
            photos.label
        )
        keepScreenshot("online-4-map-aerial")
        // And in full behind the credits button, with the map's own.
        app.buttons["map-credits"].tap()
        let credits = element("wind-credits", in: app)
        XCTAssertTrue(credits.appears(timeout: 10))
        XCTAssertTrue(credits.label.contains("Photos: "), credits.label)
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)
        // The aerial view is a switch in the layers sheet here.
        XCTAssertTrue(openLayers(in: app))
        XCTAssertTrue(isOn(layersSwitch("map-aerial", in: app)))
        setLayer("map-aerial", on: false, in: app)
    }

    /// The aerial view over Freiburg im Breisgau, from the network:
    /// Baden-Württemberg's photographs, whose source is a WMS asked for
    /// a box of each tile, close enough that the world's mosaic would be
    /// a blur. France's box reaches here too, and its photographs come
    /// through Venti: the test names a Venti where nothing listens
    /// (`nowhere`), so the French layer is in the style and IGN is named,
    /// though none of its photographs come. Its maker is named from
    /// zoom 13, where its source begins.
    func testTheAerialViewOverBadenWuerttemberg() {
        let app = launch(bundledMap: false, extra: ["--map-at", "47.995,7.852,16"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        setLayer("map-aerial", on: true, in: app)
        let photos = element("photo-credits", in: app)
        XCTAssertTrue(photos.appears(timeout: 30))
        sleep(15)
        keepScreenshot("aerial-bw-1-freiburg")
        XCTAssertEqual(photos.label, "Photos: IGN · LGL-BW · EOX")
        // In full in the credits, with the map's own.
        app.buttons["map-credits"].tap()
        let credits = element("wind-credits", in: app)
        XCTAssertTrue(credits.appears(timeout: 10))
        XCTAssertTrue(credits.label.contains("LGL-BW"), credits.label)
        app.navigationBars["Credits"].buttons["Done"].tap()
        usleep(700_000)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        setLayer("layer-sites", on: true, in: app)

        // The filter, from its button under the one that lists the sites.
        XCTAssertTrue(waitForCredit("Sites", in: app))
        XCTAssertFalse(element("sites-filter-line", in: app).exists)
        let filter = layersRow("sites-filter", in: app)
        XCTAssertTrue(filter.exists)
        filter.tap()
        let uses = element("sites-shown", in: app)
        XCTAssertTrue(uses.appears(timeout: 15))
        uses.buttons["Take-offs"].tap()
        // The sixteen points scroll: "S" is brought into the row.
        let winds = element("sites-wind", in: app)
        XCTAssertTrue(winds.appears(timeout: 10))
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
        XCTAssertTrue(leaves.appears(timeout: 10))
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
        usleep(400_000)
        keepScreenshot("sites-filter-1b-what-it-leaves")
        app.buttons["Done"].tap()

        // The map: the line of the filter with the same words, the
        // take-offs that fit as ever, the ones that say nothing muted.
        let line = element("sites-filter-line", in: app)
        XCTAssertTrue(line.appears(timeout: 15))
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
        XCTAssertTrue(app.navigationBars["Flying sites"].appears(timeout: 15))
        XCTAssertTrue(element("site-row", in: app).appears(timeout: 15))
        XCTAssertTrue(saying(fits, in: app).exists)
        XCTAssertTrue(saying(unsaid, in: app).exists)
        XCTAssertTrue(element("site-row-unsaid", in: app).exists)
        XCTAssertTrue(saying("They are shown apart, not left out.", in: app).exists)
        usleep(400_000)
        keepScreenshot("sites-filter-3-list")
        app.buttons["Done"].tap()

        // Kept as the pilot left it: the next launch, in the dark, has
        // the same filter on.
        app.terminate()
        app = launch(bundledMap: true, dark: true, extra: valley)
        let kept = element("sites-filter-line", in: app)
        XCTAssertTrue(kept.appears(timeout: 90))
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
    private func makeAccount(
        on server: String, named name: String = "Site Tester", handlePrefix: String = "sites"
    ) async throws -> (handle: String, password: String) {
        let handle = "\(handlePrefix)\(Int.random(in: 100_000...999_999))"
        let password = "made-up-\(UUID().uuidString.lowercased())"
        var request = URLRequest(url: try XCTUnwrap(URL(string: server + "/v1/accounts")))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(
            withJSONObject: ["handle": handle, "display_name": name, "password": password]
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        openAlerts(in: app)
        XCTAssertTrue(labelled(containing: "Patscherkofel north", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Nordkette cross-country", in: app).exists)
        let fired = element("alerts-fired-line", in: app)
        XCTAssertTrue(fired.appears(timeout: 30))
        XCTAssertTrue(fired.label.contains("Patscherkofel"), fired.label)
        usleep(400_000)
        keepScreenshot("alerts-1-list")

        // "Alert me" on the Zugspitze's page.
        openTab("Map", in: app)
        XCTAssertTrue(waitForCredit("Wind", in: app))
        let stations = layersRow("Wind stations", in: app, timeout: 30)
        XCTAssertTrue(stations.exists)
        stations.tap()
        XCTAssertTrue(app.navigationBars["Wind stations"].appears(timeout: 15))
        let summit = labelled(containing: "Zugspitze", in: app)
        XCTAssertTrue(summit.appears(timeout: 30))
        summit.tap()
        XCTAssertTrue(element("wind-chart", in: app).appears(timeout: 30))
        bring("alert-me", into: app).tap()
        let line = element("alert-line", in: app)
        XCTAssertTrue(line.appears(timeout: 15))
        XCTAssertTrue(line.label.hasPrefix("Wind at Zugspitze"), line.label)
        usleep(400_000)
        keepScreenshot("alerts-2-form")

        // Neither directions nor speeds: the core's fault, at its field.
        element("alert-save", in: app).tap()
        let fault = element("alert-fault-rule.directions", in: app)
        XCTAssertTrue(fault.appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("alerts-3-fault")
        let north = element("alert-from", in: app).buttons["N"].firstMatch
        XCTAssertTrue(north.appears(timeout: 10))
        north.tap()
        XCTAssertTrue(line.label.contains("from N"), line.label)
        for _ in 0..<10 where fault.exists {
            usleep(300_000)
        }
        XCTAssertFalse(fault.exists)
        element("alert-save", in: app).tap()
        // The first alarm asks whether Venti may notify.
        let allow = XCUIApplication(bundleIdentifier: "com.apple.springboard").buttons["Allow"]
        if allow.appears(timeout: 8) {
            allow.tap()
        }
        XCTAssertTrue(element("alert-me", in: app).appears(timeout: 20))
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
        XCTAssertTrue(labelled(containing: "Wind at Zugspitze from N", in: app).appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "Patscherkofel north", in: app).exists)
        usleep(400_000)
        keepScreenshot("alerts-4-list-with-the-new-one")

        // In the dark: the list, and the Patscherkofel's alarm in its form.
        // The alarms come the oldest first: the Patscherkofel's is the first.
        setAppearance(dark: true)
        let row = element("alert-row", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        XCTAssertTrue(row.label.contains("Patscherkofel north"), row.label)
        XCTAssertTrue(element("alerts-fired-line", in: app).appears(timeout: 30))
        keepScreenshot("alerts-5-list-dark")
        row.tap()
        let form = element("alert-line", in: app)
        XCTAssertTrue(form.appears(timeout: 15))
        XCTAssertTrue(form.label.hasPrefix("Wind at Patscherkofel from N to NE"), form.label)
        usleep(400_000)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        openAccount(in: app)

        // Signed out: Google first, the password form behind its button.
        let continueWithGoogle = element("account-google", in: app)
        XCTAssertTrue(continueWithGoogle.appears(timeout: 30))
        XCTAssertEqual(continueWithGoogle.label, "Continue with Google")
        XCTAssertTrue(element("account-with-password", in: app).exists)
        XCTAssertFalse(app.textFields["Handle"].exists)
        usleep(400_000)
        keepScreenshot("google-1-signed-out")

        // A new pilot: the handle step, the suggestion filled in.
        continueWithGoogle.tap()
        let handle = element("google-handle", in: app)
        XCTAssertTrue(handle.appears(timeout: 30))
        XCTAssertEqual(handle.value as? String, "ui_test")
        XCTAssertEqual(element("google-display-name", in: app).value as? String, "UI Test")
        XCTAssertTrue(labelled(containing: "@ui_test is free.", in: app).exists)
        XCTAssertTrue(element("google-handle-title", in: app).exists)
        usleep(400_000)
        keepScreenshot("google-2-handle-step")
        element("google-create", in: app).tap()

        // Signed in, with the Google email shown to the pilot.
        XCTAssertTrue(labelled(containing: "@ui_test", in: app).appears(timeout: 30))
        let shown = element("account-google-email", in: app)
        XCTAssertTrue(shown.appears(timeout: 15))
        XCTAssertTrue(shown.label.contains(email), shown.label)
        XCTAssertTrue(element("account-unlink-google", in: app).exists)
        XCTAssertTrue(bring("Sync now", into: app).exists)
        usleep(400_000)
        keepScreenshot("google-3-signed-in")

        // Signed out, and in again: no handle step.
        bring("account-sign-out", into: app).tap()
        let confirm = element("account-sign-out-confirm", in: app)
        XCTAssertTrue(confirm.appears(timeout: 10))
        confirm.tap()
        XCTAssertTrue(continueWithGoogle.appears(timeout: 30))
        continueWithGoogle.tap()
        XCTAssertTrue(element("account-google-email", in: app).appears(timeout: 30))
        XCTAssertFalse(handle.exists)
        XCTAssertTrue(labelled(containing: "@ui_test", in: app).exists)

        // In the dark, signed in by Google on a second launch.
        app.terminate()
        let dark = launch(bundledMap: true, dark: true, extra: google)
        XCTAssertTrue(dark.buttons["Show my position"].appears(timeout: 90))
        openAccount(in: dark)
        let again = element("account-google", in: dark)
        XCTAssertTrue(again.appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("google-4-signed-out-dark")
        again.tap()
        XCTAssertTrue(element("account-google-email", in: dark).appears(timeout: 30))
        XCTAssertTrue(bring("Sync now", into: dark).exists)
        usleep(400_000)
        keepScreenshot("google-5-signed-in-dark")
    }

    /// Opens Settings, Account.
    private func openAccount(in app: XCUIApplication) {
        openTab("Settings", in: app)
        let link = element("account-link", in: app)
        for _ in 0..<4 where !(link.exists && link.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(link.appears(timeout: 10))
        link.tap()
    }

    /// Opens "Alerts" under Settings, Account.
    private func openAlerts(in app: XCUIApplication) {
        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        bring("alerts-link", into: app).tap()
        XCTAssertTrue(app.navigationBars["Alerts"].appears(timeout: 15))
    }

    /// Signs in under Settings, Account, and comes back to the map.
    private func signIn(_ account: (handle: String, password: String), in app: XCUIApplication) {
        openTab("Settings", in: app)
        let link = element("account-link", in: app)
        for _ in 0..<4 where !(link.exists && link.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(link.appears(timeout: 10))
        link.tap()
        let handle = openThePasswordForm(in: app)
        XCTAssertTrue(handle.appears(timeout: 15))
        handle.tap()
        handle.typeText(account.handle)
        let password = app.secureTextFields["Password"]
        password.tap()
        password.typeText(account.password)
        element("account-submit", in: app).tap()
        XCTAssertTrue(labelled(containing: "@\(account.handle)", in: app).appears(timeout: 30))
        // The iPhone offers to keep the password. Not in a test.
        for asking in [app, XCUIApplication(bundleIdentifier: "com.apple.springboard")] {
            let notNow = asking.buttons["Not Now"]
            if notNow.appears(timeout: 3) {
                notNow.tap()
                break
            }
        }
        openTab("Map", in: app)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 30))
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
        XCTAssertTrue(entry.appears(timeout: 20))
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
        XCTAssertTrue(wanted.appears(timeout: 10), identifier)
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
        XCTAssertTrue(wanted.appears(timeout: 10), identifier)
        return wanted
    }

    /// Opens the filter of the sites from its button on the map, and
    /// brings the switch "Places from pilots" at its foot into view.
    private func openThePilotsSwitch(in app: XCUIApplication) -> XCUIElement {
        let filter = layersRow("sites-filter", in: app)
        XCTAssertTrue(filter.exists)
        filter.tap()
        XCTAssertTrue(element("sites-shown", in: app).appears(timeout: 15))
        let layer = bringInSheet("sites-from-pilots", in: app)
        _ = bringInSheet("sites-from-pilots-about", in: app)
        return layer
    }

    /// Whether a switch is on. Waits for it first: a list drawn again (the
    /// account's page after signing in) takes its rows away for a moment,
    /// and reading a switch then failed the test on the iPhone Air.
    private func isOn(_ toggle: XCUIElement) -> Bool {
        _ = toggle.waitForExistence(timeout: 10)
        return (toggle.value as? String) == "1"
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
    /// "Musterkogel West", which goes to the crowdsourced tier, and is
    /// refused a site on top of "Musterkogel West", with the offer to open
    /// that one; the form in the dark.
    func testAPilotAddsCorrectsAndRemovesASite() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let account = try await makeAccount(on: server)
        // "Musterkogel West" in the map's middle, at zoom 16, where a point
        // is some 0.8 m.
        var app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.3,10.9,16", "--all-sites"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(account, in: app)

        // Some 220 m north-west of it: a place of its own. (Within 150 m,
        // `SAME_PLACE_M`, a take-off from pilots is the partner's take-off
        // and is left out of the answer, owner decision 22.) An account
        // the test made is an ordinary pilot: the entry and the form are
        // the crowdsourced tier's, in the core's words.
        let entry = pressTheMap(dx: -140, dy: -230, in: app)
        XCTAssertEqual(entry.label, "Add a place pilots take off from")
        sleep(2)
        keepScreenshot("site-edit-1-entry-on-the-map")
        entry.tap()

        let name = element("site-name", in: app)
        XCTAssertTrue(name.appears(timeout: 15))
        XCTAssertTrue(app.navigationBars["A place pilots take off from"].exists)
        usleep(400_000)
        keepScreenshot("site-edit-2-form-empty")

        // Sent empty: the core's faults at their fields, and nothing sent.
        bring("site-send", into: app).tap()
        XCTAssertTrue(element("site-fault", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("site-edit-3-form-with-a-fault")

        name.tap()
        name.typeText("Testkogel\n")
        firstChoice("site-use", in: app).tap()
        let height = bring("site-height", into: app)
        height.tap()
        height.typeText("1500")
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
        XCTAssertTrue(said.appears(timeout: 30))
        XCTAssertTrue(said.label.contains("Added"), said.label)
        XCTAssertEqual(element("site-tier", in: app).label, "From pilots")
        // Nobody has said yet that they took off here: the core's line.
        let nobody = element("site-confirmations-none", in: app)
        XCTAssertTrue(nobody.appears(timeout: 20))
        XCTAssertTrue(nobody.label.hasPrefix("No pilot has said yet"), nobody.label)
        sleep(2)
        keepScreenshot("site-edit-5-new-site-page")
        app.buttons["Done"].firstMatch.tap()

        // Correcting it, its history, undoing and removing it, and what a
        // removed site offers: the core's (`site_offers.rs`,
        // `site_edit.rs`) and the server's (`tests/sites.rs`:
        // `a_partner_adds_corrects_undoes_and_removes_a_site_and_reads_each_step_back`,
        // `a_site_keeps_every_revision_and_the_layer_says_what_changed_since`).
        // The places from pilots switched on and the site marked in the
        // list are `testAPilotSwitchesThePlacesFromPilotsOnAndConfirmsOne`'s.

        // On top of "Musterkogel West": some twelve metres east of it,
        // beside its sign (under it lies the panel at the map's foot). The
        // server refuses, and the core names the site. The map so close
        // that a finger's width is a few metres: at zoom 19 a point is a
        // tenth of a metre. (A launch signs out.)
        app.terminate()
        app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.3,10.9,19", "--all-sites"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(account, in: app)
        pressTheMap(dx: 120, dy: -40, in: app).tap()
        let second = element("site-name", in: app)
        XCTAssertTrue(second.appears(timeout: 15))
        second.tap()
        second.typeText("Too close\n")
        firstChoice("site-use", in: app).tap()
        // The form in the dark, as it stands.
        setAppearance(dark: true)
        keepScreenshot("site-edit-form-dark-1")
        _ = bring("site-send", into: app)
        keepScreenshot("site-edit-form-dark-2-giving")
        setAppearance(dark: false)
        bring("site-send", into: app).tap()
        let inTheWay = element("site-in-the-way", in: app)
        XCTAssertTrue(inTheWay.appears(timeout: 30))
        XCTAssertEqual(inTheWay.label, "Open Musterkogel West")
        XCTAssertTrue(element("site-problem", in: app).label.contains("Musterkogel West"))
        _ = bring("site-in-the-way", into: app)
        keepScreenshot("site-edit-11-too-close")
        inTheWay.tap()
        XCTAssertTrue(app.staticTexts["Musterkogel West"].appears(timeout: 20))
        sleep(2)
        keepScreenshot("site-edit-12-the-site-in-the-way")
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
    /// core's sentence and their line among the pilots'.
    func testAPilotSwitchesThePlacesFromPilotsOnAndConfirmsOne() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        leaveTheSitesFilterAskingNothing()
        let account = try await makeAccount(on: server)
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", Self.lechtal, "--all-sites"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(account, in: app)
        setLayer("layer-sites", on: true, in: app)

        // Off: the partner's sites, and no place from pilots.
        layersRow("Flying sites", in: app).tap()
        XCTAssertTrue(labelled(containing: "Musterkogel West", in: app).appears(timeout: 30))
        XCTAssertFalse(labelled(containing: "Beispieljoch", in: app).exists)
        XCTAssertFalse(labelled(containing: "From pilots", in: app).exists)
        app.buttons["Done"].firstMatch.tap()
        sleep(1)

        // The switch at the foot of the filter, off and on.
        let layer = openThePilotsSwitch(in: app)
        XCTAssertEqual(layer.label, "Places from pilots")
        XCTAssertFalse(isOn(layer))
        usleep(400_000)
        keepScreenshot("site-tiers-1-filter-switch-off")
        set(layer, on: true)
        // The core's line under what the filter leaves.
        let among = element("sites-filter-from-pilots", in: app)
        for _ in 0..<3 where !isOnScreen(among, in: app) {
            app.swipeDown()
            usleep(600_000)
        }
        XCTAssertTrue(among.appears(timeout: 20))
        XCTAssertTrue(among.label.contains("from pilots"), among.label)
        _ = bringInSheet("sites-from-pilots-about", in: app)
        usleep(400_000)
        keepScreenshot("site-tiers-2-filter-switch-on")
        app.buttons["Done"].firstMatch.tap()

        // Counted as a filter being on: its line stands over the map.
        XCTAssertTrue(element("sites-filter-line", in: app).appears(timeout: 20))
        sleep(3)
        keepScreenshot("site-tiers-3-on-the-map")

        // In the list, marked.
        layersRow("Flying sites", in: app).tap()
        let row = labelled(containing: "Beispieljoch", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        XCTAssertTrue(row.label.contains("From pilots"), row.label)
        XCTAssertFalse(labelled(containing: "Musterkogel West", in: app).label.contains("From"))
        usleep(400_000)
        keepScreenshot("site-tiers-4-list")
        row.tap()

        // Its page: the tier by the name, one pilot's confirmation, the
        // sentence to say, and the notice.
        XCTAssertTrue(element("site-tier", in: app).appears(timeout: 20))
        XCTAssertEqual(element("site-tier", in: app).label, "From pilots")
        let count = element("site-confirmations", in: app)
        XCTAssertTrue(count.appears(timeout: 20))
        XCTAssertTrue(count.label.contains("1 pilot took off here"), count.label)
        let confirm = element("site-confirm", in: app)
        XCTAssertTrue(confirm.appears(timeout: 20))
        XCTAssertEqual(confirm.label, "I took off here and it was good")
        usleep(400_000)
        keepScreenshot("site-tiers-5-page-from-pilots")
        _ = bring("site-tier-note", into: app)
        keepScreenshot("site-tiers-6-page-notice-and-offers")

        // Said for yesterday, with a line.
        bring("site-confirm", into: app, down: true).tap()
        let day = element("site-confirm-day", in: app)
        XCTAssertTrue(day.appears(timeout: 15))
        day.tap()
        let yesterday = app.buttons["Yesterday"]
        XCTAssertTrue(yesterday.appears(timeout: 10))
        yesterday.tap()
        usleep(400_000)
        keepScreenshot("site-tiers-7-confirm-sheet")
        let note = element("site-confirm-note", in: app)
        note.tap()
        note.typeText("Smooth at noon")
        usleep(400_000)
        keepScreenshot("site-tiers-7b-confirm-sheet-with-a-line")
        bring("site-confirm-send", into: app).tap()

        // The core's sentence in the button's place, with the two ways on.
        let confirmed = element("site-confirmed", in: app)
        XCTAssertTrue(confirmed.appears(timeout: 30))
        XCTAssertTrue(confirmed.label.hasPrefix("You confirmed this site for"), confirmed.label)
        XCTAssertEqual(element("site-confirm", in: app).label, "Change the day")
        let withdraw = element("site-withdraw", in: app)
        XCTAssertEqual(withdraw.label, "Take my confirmation back")
        XCTAssertTrue(count.label.contains("2 pilots took off here"), count.label)
        usleep(400_000)
        keepScreenshot("site-tiers-8-page-after-confirming")

        // "Pilots who took off here", with the pilot's own line.
        count.tap()
        XCTAssertTrue(app.navigationBars["Pilots who took off here"].appears(timeout: 15))
        let mine = labelled(containing: "Smooth at noon", in: app)
        XCTAssertTrue(mine.appears(timeout: 20))
        XCTAssertTrue(mine.label.contains("@\(account.handle)"), mine.label)
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "site-confirmation").count, 2)
        usleep(400_000)
        keepScreenshot("site-tiers-9-pilots-who-took-off-here")
        app.navigationBars["Pilots who took off here"].buttons.firstMatch.tap()

        // Taking the confirmation back, and a partner's site that offers
        // no correcting: the server's (`tests/sites.rs`,
        // `a_pilot_says_they_took_off_at_a_site_and_takes_it_back`) and the
        // core's (`site_offers.rs`).
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        setLayer("layer-sites", on: true, in: app)
        set(openThePilotsSwitch(in: app), on: true)
        usleep(400_000)
        keepScreenshot("site-tiers-dark-1-filter-switch-on")
        app.buttons["Done"].firstMatch.tap()
        layersRow("Flying sites", in: app).tap()
        let row = labelled(containing: "Beispieljoch", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        row.tap()
        XCTAssertTrue(element("site-tier", in: app).appears(timeout: 20))
        XCTAssertTrue(element("site-confirmations", in: app).appears(timeout: 20))
        // Signed out: nothing to say or correct, and the line says why.
        XCTAssertFalse(element("site-confirm", in: app).exists)
        usleep(400_000)
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
        XCTAssertTrue(entry.appears(timeout: 20))
        XCTAssertEqual(entry.label, "Sign in to add a place pilots take off from.")
        XCTAssertFalse(element("add-site-here", in: app).exists)
        sleep(2)
        keepScreenshot("site-tiers-dark-4-entry-signed-out")
        entry.tap()
        XCTAssertTrue(
            app.textFields["Handle"].appears(timeout: 15)
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
        let app = launch(
            bundledMap: true,
            extra: [
                "--server", server, "--map-at", Self.lechtal, "--all-sites",
                "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXXXL",
            ]
        )
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(account, in: app)
        setLayer("layer-sites", on: true, in: app)

        // The filter's sheet, the switch at its foot off and on.
        let layer = openThePilotsSwitch(in: app)
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-1-filter-switch-off", dark: "site-tiers-larger-text-dark-1-filter-switch-off")
        set(layer, on: true)
        sleep(2)
        keepScreenshots(light: "site-tiers-larger-text-light-2-filter-switch-on", dark: "site-tiers-larger-text-dark-2-filter-switch-on")
        // Back to the sheet's head. A swipe down at its head puts the
        // sheet away, which is as good as "Done".
        for _ in 0..<2 where app.buttons["Done"].firstMatch.exists {
            app.swipeDown()
            usleep(500_000)
        }
        keepScreenshots(light: "site-tiers-larger-text-light-2b-filter-top", dark: "site-tiers-larger-text-dark-2b-filter-top")
        let done = app.buttons["Done"].firstMatch
        if done.appears(timeout: 3) {
            done.tap()
        }
        XCTAssertTrue(element("sites-filter-line", in: app).appears(timeout: 20))
        sleep(3)
        keepScreenshots(light: "site-tiers-larger-text-light-3-line-over-the-map", dark: "site-tiers-larger-text-dark-3-line-over-the-map")

        // The list and a place from pilots' page.
        layersRow("Flying sites", in: app).tap()
        let row = labelled(containing: "Beispieljoch", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-4-list", dark: "site-tiers-larger-text-dark-4-list")
        row.tap()
        XCTAssertTrue(element("site-tier", in: app).appears(timeout: 20))
        let count = element("site-confirmations", in: app)
        XCTAssertTrue(count.appears(timeout: 20))
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-5-page-head", dark: "site-tiers-larger-text-dark-5-page-head")
        let confirm = bring("site-confirm", into: app)
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-6-page-confirm-entry", dark: "site-tiers-larger-text-dark-6-page-confirm-entry")

        // The sheet to confirm in.
        confirm.tap()
        XCTAssertTrue(element("site-confirm-day", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-7-confirm-sheet", dark: "site-tiers-larger-text-dark-7-confirm-sheet")
        _ = bring("site-confirm-send", into: app)
        keepScreenshots(light: "site-tiers-larger-text-light-7b-confirm-sheet-foot", dark: "site-tiers-larger-text-dark-7b-confirm-sheet-foot")
        app.buttons["Cancel"].firstMatch.tap()
        sleep(1)

        // The pilots who took off here.
        bring("site-confirmations", into: app, down: true).tap()
        XCTAssertTrue(element("site-confirmation", in: app).appears(timeout: 20))
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-8-pilots-who-took-off-here", dark: "site-tiers-larger-text-dark-8-pilots-who-took-off-here")
        app.navigationBars.buttons.firstMatch.tap()

        // The notice and what the core offers, at the page's foot.
        _ = bring("site-tier-note", into: app)
        keepScreenshots(light: "site-tiers-larger-text-light-9-page-notice", dark: "site-tiers-larger-text-dark-9-page-notice")
        _ = bring("site-history", into: app)
        keepScreenshots(light: "site-tiers-larger-text-light-10-page-offers", dark: "site-tiers-larger-text-dark-10-page-offers")
        app.navigationBars.buttons.firstMatch.tap()
        app.buttons["Done"].firstMatch.tap()
        sleep(2)

        // The form for a new place, empty, with its faults, and its foot.
        let entry = pressTheMap(dx: -140, dy: -230, in: app)
        sleep(2)
        keepScreenshots(light: "site-tiers-larger-text-light-11-entry-on-the-map", dark: "site-tiers-larger-text-dark-11-entry-on-the-map")
        entry.tap()
        XCTAssertTrue(element("site-name", in: app).appears(timeout: 15))
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-12-form", dark: "site-tiers-larger-text-dark-12-form")
        bring("site-send", into: app).tap()
        XCTAssertTrue(element("site-fault", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshots(light: "site-tiers-larger-text-light-13-form-faults", dark: "site-tiers-larger-text-dark-13-form-faults")
        for step in 1...4 {
            app.swipeUp()
            usleep(700_000)
            keepScreenshots(light: "site-tiers-larger-text-light-14-form-further-\(step)", dark: "site-tiers-larger-text-dark-14-form-further-\(step)")
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(account, in: app)

        // The Jägerkamp, one of Today's rows, kept on its page.
        // The settings are a page of the Today tab, and the sign-in left
        // the account's page open there: a second tap goes back to Today.
        openTab("Today", in: app)
        app.tabBars.buttons["Today"].tap()
        XCTAssertTrue(element("today-card", in: app).appears(timeout: 60))
        // Wherever the core's order puts it on the day: a row, or the card.
        let row = app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier IN %@", ["today-row", "today-card"]))
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(row.appears(timeout: 30))
        bringUp(row, in: app)
        row.tap()
        let keep = element("site-keep", in: app)
        bringUp(keep, in: app)
        XCTAssertTrue(keep.appears(timeout: 10))
        keep.tap()
        XCTAssertTrue(app.buttons["Kept"].appears(timeout: 10))
        // Sent to the account at once: the core's note says where it lives.
        XCTAssertTrue(
            labelled(containing: "Kept on your Venti account and on this device", in: app)
                .appears(timeout: 30)
        )
        usleep(400_000)
        keepScreenshot("kept-account-1-site-kept")

        // A second start: every start of a UI test has an empty folder of
        // its own and is signed out, as a second iPhone would be.
        app.terminate()
        let second = launch(bundledMap: true, extra: arguments)
        XCTAssertTrue(second.buttons["Show my position"].appears(timeout: 90))
        openTab("Settings", in: second)
        let link = element("kept-sites-link", in: second)
        XCTAssertTrue(link.appears(timeout: 10))
        link.tap()
        XCTAssertTrue(second.staticTexts["No take-offs of your own yet"].appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Kept on this device only", in: second).exists)

        // Signed in to the same account, the take-off comes back from it.
        signIn(account, in: second)
        openTab("Settings", in: second)
        XCTAssertTrue(link.appears(timeout: 10))
        link.tap()
        let listed = second.descendants(matching: .any).matching(identifier: "kept-site-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "Jägerkamp")).firstMatch
        XCTAssertTrue(listed.appears(timeout: 30))
        XCTAssertTrue(
            labelled(containing: "Kept on your Venti account and on this device", in: second)
                .appears(timeout: 15)
        )
        usleep(400_000)
        keepScreenshot("kept-account-2-back-from-the-account")
    }

    /// Opens "Programs with access" from the account's page, signed in,
    /// and keeps a screenshot of the account's page on the way if asked.
    private func openPrograms(in app: XCUIApplication, keeping screenshot: String? = nil) {
        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        let link = bring("programs-link", into: app)
        if let screenshot {
            usleep(400_000)
            keepScreenshot(screenshot)
        }
        link.tap()
        XCTAssertTrue(app.navigationBars["Programs with access"].appears(timeout: 15))
    }

    /// A pilot gives "My logger" access with two things it may do, sees
    /// its secret once and finds it in the list, with the button that
    /// withdraws it; the list and the form in the dark too.
    func testAPilotGivesAProgramAccessAndWithdrawsIt() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let account = try await makeAccount(on: server)
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(account, in: app)

        // The account's page has the row; a new account's list is empty.
        openPrograms(in: app, keeping: "programs-1-account")
        let empty = element("programs-empty", in: app)
        XCTAssertTrue(empty.appears(timeout: 30))
        XCTAssertEqual(empty.label, "No program has access to your account.")
        usleep(400_000)
        keepScreenshots(light: "programs-2-empty", dark: "programs-dark-1-empty")

        // The form: sent with nothing in it, it says what is missing.
        element("programs-give", in: app).tap()
        let name = element("token-name", in: app)
        XCTAssertTrue(name.appears(timeout: 15))
        usleep(400_000)
        keepScreenshots(light: "programs-3-form", dark: "programs-dark-2-form")
        _ = bring("token-submit", into: app)
        setAppearance(dark: true)
        keepScreenshot("programs-dark-3-form-end")
        setAppearance(dark: false)
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
        usleep(400_000)
        keepScreenshot("programs-4-form-two-chosen")
        _ = bring("token-password", into: app)
        let password = app.secureTextFields["token-password"]
        password.tap()
        password.typeText(account.password + "\n")
        _ = bring("token-submit", into: app)
        usleep(400_000)
        keepScreenshot("programs-5-form-password")
        element("token-submit", in: app).tap()

        // The secret, this once, with the core's words; copied.
        let secret = element("token-secret", in: app)
        XCTAssertTrue(secret.appears(timeout: 30))
        // Not written into the test's log either: only its form is looked at.
        XCTAssertTrue(secret.label.hasPrefix("venti_pat_"))
        XCTAssertEqual(element("token-shown-once", in: app).label, "Copy it now: Venti does not show it again.")
        XCTAssertFalse(app.buttons["Cancel"].exists)
        let copy = element("token-copy", in: app)
        copy.tap()
        XCTAssertEqual(copy.value as? String, "Copied")
        usleep(400_000)
        keepScreenshot("programs-6-secret-once")
        let done = bring("token-done", into: app)
        XCTAssertEqual(done.label, "I have copied it")
        done.tap()

        // The list has it, with what it may do; the secret is nowhere.
        let listed = app.descendants(matching: .any).matching(identifier: "program-row")
            .matching(NSPredicate(format: "label CONTAINS %@", "My logger")).firstMatch
        XCTAssertTrue(listed.appears(timeout: 15))
        XCTAssertTrue(listed.label.contains("Flights: write"), listed.label)
        XCTAssertTrue(listed.label.contains("Live tracking: send"), listed.label)
        XCTAssertFalse(secret.exists)
        XCTAssertFalse(labelled(containing: "venti_pat_", in: app).exists)
        XCTAssertFalse(empty.exists)
        usleep(400_000)
        keepScreenshot("programs-7-list")

        // Withdrawing it, after the core's question: the server's
        // (`tests/tokens.rs`, `a_pilot_makes_a_token_sees_it_listed_and_withdraws_it`).
        XCTAssertTrue(bring("program-withdraw", into: app).exists)
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
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
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
        XCTAssertTrue(words.appears(timeout: 20))
        XCTAssertTrue(words.label.contains("public flight"), words.label)
        let months = app.descendants(matching: .any).matching(identifier: "flown-month")
        if words.label.hasPrefix("No public flight") {
            XCTAssertEqual(months.count, 0)
        } else {
            XCTAssertEqual(months.count, 12)
            XCTAssertTrue(months.element(boundBy: 0).label.hasPrefix("January"), months.element(boundBy: 0).label)
        }
        usleep(400_000)
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
        usleep(400_000)
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
            _ = place.appears(timeout: 8)
        }
        XCTAssertTrue(place.appears(timeout: 10))
        XCTAssertTrue(place.label.hasPrefix("Pilots take off here"), place.label)
        sleep(2)
        keepScreenshot("flights-at-sites-5-candidate-sheet")
        // Under the place, in a list that has only the rows in view.
        let offer = bringInSheet("candidate-offer-words", in: app)
        XCTAssertTrue(offer.label.hasPrefix("You took off here on"), offer.label)
        _ = bringInSheet("candidate-set-aside", in: app)
        XCTAssertEqual(element("candidate-add", in: app).label, "Add this place")
        usleep(400_000)
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
        XCTAssertTrue(dark.buttons["Show my position"].appears(timeout: 90))
        setLayer("layer-sites", on: true, in: dark)
        openMusterkogelWest(in: dark)
        _ = bring("site-flights", into: dark)
        usleep(400_000)
        keepScreenshot("flights-at-sites-dark-site-section")
    }

    /// Opens the made-up take-off "Musterkogel West" from the list of the
    /// sites.
    private func openMusterkogelWest(in app: XCUIApplication) {
        let list = layersRow("Flying sites", in: app)
        XCTAssertTrue(list.exists)
        list.tap()
        let row = labelled(containing: "Musterkogel West", in: app)
        XCTAssertTrue(row.appears(timeout: 30))
        row.tap()
        XCTAssertTrue(element("site-plan-route", in: app).appears(timeout: 20))
    }

    // MARK: The parity of 2026-11-22

    /// The logbook brings in from a watched folder by itself. A test
    /// cannot drive the system's folder picker, so the app starts with a
    /// folder of its own named (`--watched-folder`: "Tracklogs", with the
    /// sample flight in it), through the same `watch` the picker ends in:
    /// the flight is in the logbook without a tap, the menu names the
    /// folder and stops watching it.
    func testTheLogbookBringsInFromAWatchedFolder() {
        let app = launch(bundledMap: true, extra: ["--watched-folder"])
        openTab("Logbook", in: app)
        // The flight came in by itself.
        XCTAssertTrue(element("logbook-row", in: app).appears(timeout: 30))

        let menu = app.buttons["logbook-import"]
        XCTAssertTrue(menu.appears(timeout: 10))
        menu.tap()
        let stop = app.buttons["Stop bringing in from Tracklogs"]
        XCTAssertTrue(stop.appears(timeout: 10))
        XCTAssertTrue(app.buttons["Bring in from another folder than Tracklogs"].exists)
        XCTAssertTrue(app.buttons["Flight files or a folder"].exists)
        usleep(400_000)
        keepScreenshot("watched-1-menu")

        // Stopped: the flight stays, and the menu offers a folder again.
        stop.tap()
        usleep(800_000)
        XCTAssertTrue(element("logbook-row", in: app).exists)
        menu.tap()
        XCTAssertTrue(app.buttons["Bring in new flights from a folder"].appears(timeout: 10))
        XCTAssertFalse(app.buttons["Stop bringing in from Tracklogs"].exists)
        usleep(400_000)
        keepScreenshot("watched-2-menu-stopped")
    }

    /// An import of many files fills the list as it runs. A test cannot
    /// drive the system's picker, so the app starts importing a folder of
    /// its own (`--import-folder`: five copies of the sample flight on
    /// other days, four seconds apart): the bar counts the files, and the
    /// first flight is in the list while the import still runs, in place
    /// of "No flights yet".
    func testTheLogbookFillsWhileAnImportRuns() {
        let app = launch(bundledMap: true, extra: ["--import-folder"])
        openTab("Logbook", in: app)
        let progress = element("import-progress", in: app)
        XCTAssertTrue(progress.appears(timeout: 30))
        XCTAssertTrue((progress.value as? String)?.contains("of 5") == true, "\(String(describing: progress.value))")
        XCTAssertTrue(element("logbook-row", in: app).appears(timeout: 10))
        XCTAssertTrue(progress.exists, "The import ended before the list was seen filling.")
        let counted = progress.value as? String ?? ""
        XCTAssertFalse(counted.hasPrefix("5 of"), "The list was first seen with the last file: \(counted)")
        XCTAssertFalse(app.staticTexts["No flights yet"].exists)
        usleep(400_000)
        keepScreenshot("logbook-import-filling")

        XCTAssertTrue(progress.vanishes(timeout: 30))
        XCTAssertTrue(element("logbook-row", in: app).exists)
    }

    /// A watched folder that is gone: the app starts with the folder of
    /// `--watched-folder-gone`, which is deleted once its flight is in,
    /// so that the system's own bookmark leads nowhere. The logbook says
    /// so over the flights, offers to choose the folder again, and stops
    /// when told to.
    func testTheLogbookSaysWhenTheWatchedFolderIsGone() {
        let app = launch(bundledMap: true, extra: ["--watched-folder-gone"])
        openTab("Logbook", in: app)
        XCTAssertTrue(element("logbook-row", in: app).appears(timeout: 30))
        let lost = element("watched-folder-lost", in: app)
        XCTAssertTrue(lost.appears(timeout: 15))
        XCTAssertTrue(lost.label.contains("Tracklogs"), lost.label)
        XCTAssertTrue(app.buttons["Choose the folder again"].exists)
        usleep(400_000)
        keepScreenshot("watched-3-folder-gone")

        // Given up: the notice goes, the flight stays, and the menu
        // offers a folder as if none had been named.
        app.buttons["watched-folder-stop"].tap()
        usleep(800_000)
        XCTAssertFalse(lost.exists)
        XCTAssertTrue(element("logbook-row", in: app).exists)
        app.buttons["logbook-import"].tap()
        XCTAssertTrue(app.buttons["Bring in new flights from a folder"].appears(timeout: 10))
    }

    /// The account's page says what the last sync did, in the core's
    /// words: the demo pilot's three flights fetched after signing in.
    func testTheAccountSaysWhatTheLastSyncDid() throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let app = launch(bundledMap: true, extra: ["--server", server])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(("demo", "demo-pilot-password"), in: app)

        openTab("Settings", in: app)
        bring("account-link", into: app).tap()
        XCTAssertTrue(app.buttons["Sync now"].appears(timeout: 60))
        _ = bring("account-last-sync", into: app)
        // The row's name, and beside it the core's words for what the
        // last run did: the three flights fetched after signing in, or,
        // once the run that follows a changed logbook is through, that
        // there was nothing to do.
        func says(_ words: String) -> XCUIElement {
            app.descendants(matching: .any)
                .matching(NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", words, words))
                .firstMatch
        }
        XCTAssertTrue(says("Last sync").appears(timeout: 10))
        XCTAssertTrue(says("3 fetched").exists || says("Nothing to send or fetch").exists)
        usleep(400_000)
        keepScreenshot("account-last-sync")

        // A sync with nothing to do says so.
        app.buttons["Sync now"].tap()
        XCTAssertTrue(says("Nothing to send or fetch").appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("account-last-sync-nothing")
    }

    /// Who sees the pilot's flights, and what is said with them, on the
    /// account's page: an account the test makes, signed in with five
    /// sample flights and the app's glider set to EN B (the launch
    /// argument sets the preference for this launch only), which the
    /// account takes before the flights go up. The switch for new flights
    /// is switched off and on again; "Make all private" and "Make all
    /// public" ask the core's question and say the answer's line; the
    /// pilot's country is chosen from the searchable list. The pilot's
    /// page among everyone's flights then shows the flag by the name and
    /// the class beside the glider, and so does a flight's head. At the
    /// end all flights are private again, so that the tests after it on
    /// the same server see everyone's flights as they were. Then the
    /// account and the list of countries in the dark, and the account
    /// with larger text.
    func testThePilotChoosesWhoSeesTheirFlightsAndTheirCountry() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        let pilot = try await makeAccount(on: server, named: "Flag Tester", handlePrefix: "flags")
        let gliderB = ["-VentiGliderClass", "b"]
        let app = launch(bundledMap: true, extra: ["--server", server, "--sample-logbook"] + gliderB)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn(pilot, in: app)

        openAccount(in: app)
        XCTAssertTrue(app.buttons["Sync now"].appears(timeout: 60))
        XCTAssertTrue(labelled(containing: "Flights on Venti", in: app).appears(timeout: 10))
        // The sentence that flights stay private until shared is gone.
        XCTAssertFalse(labelled(containing: "until you share", in: app).exists)
        let newFlights = app.switches["account-new-flights-public"]
        _ = bring("account-new-flights-public", into: app)
        // Clear of the tab bar: the picture stands over the account now.
        bringUp(newFlights, in: app)
        XCTAssertTrue(isOn(newFlights))
        XCTAssertTrue(labelled(containing: "Switch off to keep new flights to yourself.", in: app).exists)
        usleep(400_000)
        keepScreenshot("who-sees-1-account")

        // Off, as Venti answers, and on again.
        set(newFlights, on: false)
        sleep(2)
        XCTAssertFalse(isOn(newFlights))
        keepScreenshot("who-sees-2-new-flights-private")
        set(newFlights, on: true)
        sleep(2)
        XCTAssertTrue(isOn(newFlights))

        // All private: the core's question, then the answer's line.
        bring("account-all-private", into: app).tap()
        let question = app.alerts.firstMatch
        XCTAssertTrue(question.appears(timeout: 10))
        XCTAssertTrue(question.label.contains("Make all 5 flights private?"), question.label)
        usleep(400_000)
        keepScreenshot("who-sees-3-question")
        question.buttons["Make all private"].firstMatch.tap()
        let line = element("account-all-flights-line", in: app)
        XCTAssertTrue(line.appears(timeout: 30))
        XCTAssertTrue(labelled(containing: "5 flights are private", in: app).exists)
        usleep(400_000)
        keepScreenshot("who-sees-4-all-private")

        // The pilot's country, found by part of its name.
        bring("account-country", into: app, down: true).tap()
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.appears(timeout: 15))
        XCTAssertTrue(element("country-none", in: app).exists)
        usleep(400_000)
        keepScreenshot("who-sees-5-countries")
        field.tap()
        field.typeText("Germ")
        let germany = element("country-DE", in: app)
        XCTAssertTrue(germany.appears(timeout: 10))
        keepScreenshot("who-sees-6-country-found")
        germany.tap()
        let country = element("account-country", in: app)
        XCTAssertTrue(country.appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "Germany", in: app).appears(timeout: 20))

        // All public again, for everyone's flights to show.
        bring("account-all-public", into: app).tap()
        XCTAssertTrue(question.appears(timeout: 10))
        XCTAssertTrue(question.label.contains("Make all 5 flights public?"), question.label)
        question.buttons["Make all public"].firstMatch.tap()
        XCTAssertTrue(labelled(containing: "5 flights are public", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("who-sees-7-all-public")

        // The pilot's page among everyone's flights: the flag by the
        // name, which VoiceOver reads as the country, and the class.
        openTab("Community", in: app)
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.appears(timeout: 15))
        search.tap()
        search.typeText("Flag Tester")
        let found = labelled(containing: "Flag Tester (@\(pilot.handle))", in: app)
        XCTAssertTrue(found.appears(timeout: 30))
        found.tap()
        // The flights stand under the pilot's totals.
        XCTAssertTrue(element("pilot-total", in: app).appears(timeout: 30))
        let row = bring("pilot-flight-row", into: app)
        XCTAssertTrue(row.label.contains("Germany"), row.label)
        XCTAssertTrue(row.label.contains("EN B"), row.label)
        usleep(400_000)
        keepScreenshot("who-sees-8-pilots-page")
        row.tap()
        XCTAssertTrue(element("public-flight-pilot", in: app).appears(timeout: 30))
        XCTAssertTrue(element("public-flight-pilot-country", in: app).exists)
        XCTAssertTrue(labelled(containing: "EN B", in: app).exists)
        sleep(3)
        keepScreenshot("who-sees-9-flight")

        // Private again, as everyone's flights were before. The flight's
        // page hides the tab bar: back to the pilot's page first.
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(app.tabBars.buttons["Today"].appears(timeout: 15))
        openAccount(in: app)
        bring("account-all-private", into: app).tap()
        XCTAssertTrue(question.appears(timeout: 10))
        question.buttons["Make all private"].firstMatch.tap()
        XCTAssertTrue(labelled(containing: "5 flights are private", in: app).appears(timeout: 30))

        // The account and the countries in the dark, as they stand.
        setAppearance(dark: true)
        _ = bring("account-country", into: app, down: true)
        usleep(400_000)
        keepScreenshot("who-sees-dark-1-account")
        _ = bring("account-all-private", into: app)
        usleep(400_000)
        keepScreenshot("who-sees-dark-2-who-sees")
        bring("account-country", into: app, down: true).tap()
        XCTAssertTrue(app.searchFields.firstMatch.appears(timeout: 15))
        usleep(400_000)
        keepScreenshot("who-sees-dark-3-countries")
        setAppearance(dark: false)

        // The account with larger text: a fresh start, signed out, that
        // signs in again.
        guard Self.largerTextAsked else { return }
        app.terminate()
        let again = launch(bundledMap: true, extra: ["--server", server] + gliderB + Self.largerText)
        XCTAssertTrue(again.buttons["Show my position"].appears(timeout: 90))
        signIn(pilot, in: again)
        openAccount(in: again)
        XCTAssertTrue(labelled(containing: "Germany", in: again).appears(timeout: 30))
        usleep(400_000)
        keepScreenshot("who-sees-larger-1-account")
        _ = bring("account-all-private", into: again)
        usleep(400_000)
        keepScreenshot("who-sees-larger-2-who-sees")
    }

    /// A tap on a notification of what an alarm fired opens "Alerts",
    /// also when the tap started the app. A test cannot post and tap a
    /// notification reliably, so the app starts as if by such a tap
    /// (`--tapped-alert`), through the tap's own handler.
    func testATapOnAnAlarmsNotificationOpensAlerts() {
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = Self.naming(["--ui-testing", "--bundled-map", "--tapped-alert"])
        app.launch()
        XCTAssertTrue(app.navigationBars["Alerts"].appears(timeout: 30))
        XCTAssertTrue(app.tabBars.buttons["Today"].isSelected)
        usleep(400_000)
        keepScreenshot("alerts-from-a-notification")
        // And back from it is Today.
        app.navigationBars["Alerts"].buttons.firstMatch.tap()
        XCTAssertTrue(app.navigationBars.buttons["Settings"].appears(timeout: 15))
    }

    /// The map's one choice of what it shows and the layers sheet: the
    /// chips are the wind, the thermals, the rain and the cloud, one at a
    /// time, the chosen one pressed again showing nothing; the sheet has
    /// the sites and the other layers laid over the map as switches, and
    /// none of the chips. Whatever was chosen, the wind is again at the end.
    func testTheLayersSheetHasTheChipsFour() {
        let app = launch(bundledMap: true)
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        let chips = ["layer-wind", "what-thermals", "what-rain"]
        showWhat("layer-wind", in: app)
        for chip in chips.dropFirst() {
            showWhat(chip, in: app)
            for other in chips where other != chip {
                XCTAssertFalse(element(other, in: app).isSelected, "\(other) beside \(chip)")
            }
        }
        // The chosen one pressed again: nothing.
        element("what-rain", in: app).tap()
        usleep(700_000)
        for chip in chips {
            XCTAssertFalse(element(chip, in: app).isSelected, chip)
        }
        XCTAssertFalse(element("what-panel", in: app).exists)
        usleep(400_000)
        keepScreenshot("layers-four-1-nothing-chosen")
        showWhat("layer-wind", in: app)

        XCTAssertTrue(openLayers(in: app))
        XCTAssertTrue(app.switches["layer-sites"].appears(timeout: 10))
        XCTAssertTrue(app.switches["layer-airspace"].exists)
        for chip in chips + ["layers-forecast", "layers-radar", "layers-wind"] {
            XCTAssertFalse(app.switches[chip].exists, chip)
        }
        usleep(400_000)
        keepScreenshot("layers-four-2-sheet")
        // The sites are a switch: off, the credits name none.
        let sites = app.switches["layer-sites"]
        let before = isOn(sites)
        set(sites, on: !before)
        XCTAssertEqual(isOn(sites), !before)
        set(sites, on: before)
        closeLayers(in: app)
    }

    /// "Data and licences" names the typeface, the icons, what the app is
    /// built with and the bridge to the core, each with its licence,
    /// under the map's, the photographs', the wind's and Venti's own.
    func testDataAndLicencesNameTheTypefaceAndTheLibraries() {
        let app = launch(bundledMap: true)
        openTab("Settings", in: app)
        let licences = app.buttons["Data and licences"]
        for _ in 0..<4 where !(licences.exists && licences.isHittable) {
            app.swipeUp()
        }
        licences.tap()
        XCTAssertTrue(app.navigationBars["Data and licences"].appears(timeout: 15))
        XCTAssertTrue(labelled(containing: "OpenStreetMap", in: app).appears(timeout: 10))
        usleep(400_000)
        keepScreenshot("licences-1-map")

        // The libraries are the page's last section.
        let bridge = bring("credit-bridge", into: app)
        XCTAssertTrue(bridge.exists)
        XCTAssertTrue(labelled(containing: "UniFFI, under the Mozilla Public License 2.0", in: app).exists)
        XCTAssertTrue(labelled(containing: "Fira Sans", in: app).exists)
        XCTAssertTrue(labelled(containing: "SIL Open Font License 1.1", in: app).exists)
        XCTAssertTrue(labelled(containing: "Venti's own flying icons", in: app).exists)
        XCTAssertTrue(labelled(containing: "SwiftUI", in: app).exists)
        usleep(400_000)
        keepScreenshot("licences-2-libraries")
    }
}

/// What pilots nearby do for each other: the account's two settings, and
/// help after landing between two pilots, one in the app and one over the
/// development server's API (docs/design.md, "Help after landing"). What
/// the server decides (whom a call goes to, who sees where) is held by the
/// server's tests (`crates/server/tests/help.rs`) and the answers' words by
/// the core's (`phones.rs`); these only see the screens show them.
extension VentiUITests {
    /// Where the simulated iPhone stands: by the Inn at Völs.
    private static let vols = CLLocation(latitude: 47.2731, longitude: 11.3962)

    /// Makes an account over the API and returns its handle, password and
    /// session token. Made up here, for this run's server.
    private func makePilot(on server: String, named name: String) async throws -> (handle: String, password: String, token: String) {
        let handle = "help\(Int.random(in: 100_000...999_999))"
        let password = "made-up-\(UUID().uuidString.lowercased())"
        let answer = try await call(
            server, "POST", "/v1/accounts", body: ["handle": handle, "display_name": name, "password": password]
        )
        XCTAssertTrue((200..<300).contains(answer.status), "The development server made no account: \(answer.status)")
        let token = try XCTUnwrap(answer.json["token"] as? String)
        return (handle, password, token)
    }

    /// One request to the development server's API, with a pilot's token
    /// where one is given.
    @discardableResult
    private func call(
        _ server: String, _ method: String, _ path: String, token: String? = nil, body: [String: Any]? = nil
    ) async throws -> (status: Int, json: [String: Any]) {
        var request = URLRequest(url: try XCTUnwrap(URL(string: server + path)))
        request.httpMethod = method
        if let token {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, json)
    }

    /// Answers what the system asks while a switch is turned on: whether
    /// Venti may know where the iPhone is, and may notify.
    private func allowWhatTheSystemAsks() {
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        for _ in 0..<3 {
            let position = springboard.buttons["Allow While Using App"]
            let notify = springboard.buttons["Allow"]
            if position.appears(timeout: 3) {
                position.tap()
            } else if notify.exists {
                notify.tap()
            } else {
                return
            }
            usleep(500_000)
        }
    }

    /// Turns a switch of "Pilots nearby" on or off: one tap, what the system
    /// asks answered, and the switch as Venti kept it, which comes only
    /// once Venti has answered.
    private func switchNearby(_ toggle: XCUIElement, on: Bool, in app: XCUIApplication) {
        if isOn(toggle) != on {
            clearOfTheTabBar(toggle, in: app)
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.93, dy: 0.5)).tap()
        }
        allowWhatTheSystemAsks()
        XCTAssertTrue(waitUntil(timeout: 20) { self.isOn(toggle) == on }, toggle.identifier)
    }

    /// Scrolls a list a little until a row stands wholly above the tab bar,
    /// which floats over the list's foot. `bring` takes a row under the
    /// tab bar for on the screen: on the iPhone Air "nearby-help" lay
    /// there, and the tap at its switch opened the Community tab.
    private func clearOfTheTabBar(_ row: XCUIElement, in app: XCUIApplication) {
        let bar = app.tabBars.firstMatch
        let window = app.windows.firstMatch
        for _ in 0..<4 where bar.exists && row.exists && row.frame.maxY > bar.frame.minY - 8 {
            window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.6))
                .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4)))
            usleep(600_000)
        }
    }

    /// Opens the account's page and brings "Pilots nearby" in.
    private func openPilotsNearby(in app: XCUIApplication) -> XCUIElement {
        openAccount(in: app)
        let help = bring("nearby-help", into: app)
        XCTAssertTrue(help.appears(timeout: 20))
        return help
    }

    /// The two settings: both off for a new account; "New open plans near"
    /// on shows where (the map's middle) and how far, a distance chosen is
    /// kept by Venti; "Pilots nearby may ask me for help" on asks for the
    /// position and to notify. Light and dark.
    func testTheSettingsForPilotsNearby() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        XCUIDevice.shared.location = XCUILocation(location: Self.vols)
        let pilot = try await makePilot(on: server, named: "Nearby Tester")
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.27,11.39,10"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn((pilot.handle, pilot.password), in: app)

        let help = openPilotsNearby(in: app)
        let plans = element("nearby-open-plans", in: app)
        XCTAssertTrue(plans.exists)
        XCTAssertFalse(isOn(help))
        XCTAssertFalse(isOn(plans))
        XCTAssertTrue(labelled(containing: "Pilots nearby may ask me for help", in: app).exists)
        usleep(400_000)
        keepScreenshot("nearby-1-settings-off")

        switchNearby(plans, on: true, in: app)
        let place = element("nearby-place", in: app)
        XCTAssertTrue(place.appears(timeout: 20))
        XCTAssertTrue(place.label.hasPrefix("Around 47.2"), place.label)
        app.buttons["50 km"].firstMatch.tap()
        sleep(2)
        // The place and the distances pushed the second switch down.
        _ = bring("nearby-help", into: app)
        switchNearby(help, on: true, in: app)
        _ = bring("nearby-help", into: app)
        usleep(400_000)
        keepScreenshots(light: "nearby-2-settings-on", dark: "nearby-3-settings-on-dark")

        // Venti keeps them whole.
        let kept = try await call(server, "GET", "/v1/account/nearby", token: pilot.token)
        XCTAssertEqual(kept.json["may_be_asked_for_help"] as? Bool, true)
        let circle = try XCTUnwrap(kept.json["open_plans_near"] as? [String: Any])
        XCTAssertEqual(circle["radius_km"] as? Int, 50)

        // Off again, both.
        switchNearby(help, on: false, in: app)
        _ = bring("nearby-open-plans", into: app, down: true)
        switchNearby(plans, on: false, in: app)
        XCTAssertTrue(place.vanishes(timeout: 20))
        app.resetAuthorizationStatus(for: .location)
    }

    /// Help after landing between two pilots. The pilot in the app may be
    /// asked; a pilot over the API, near by, asks: the call is in
    /// Community, with "I can help", after which the app shows where the
    /// caller is, and the caller sees who can help. Then the pilot in the
    /// app asks from Community, with a note, and is answered over the API:
    /// the helper's name stands on the call, and "I'm fine, close" ends it.
    /// Light and dark.
    func testPilotsNearbyAskForHelpAndAnswer() async throws {
        guard let server = ProcessInfo.processInfo.environment["VENTI_SERVER"], !server.isEmpty else {
            throw XCTSkip("No Venti server was named in VENTI_SERVER.")
        }
        XCUIDevice.shared.location = XCUILocation(location: Self.vols)
        let anna = try await makePilot(on: server, named: "Anna Caller")
        let bert = try await makePilot(on: server, named: "Bert Helper")
        let app = launch(bundledMap: true, extra: ["--server", server, "--map-at", "47.27,11.39,10"])
        XCTAssertTrue(app.buttons["Show my position"].appears(timeout: 90))
        signIn((bert.handle, bert.password), in: app)

        // Bert may be asked: the app tells Venti where the iPhone is.
        let help = openPilotsNearby(in: app)
        switchNearby(help, on: true, in: app)

        // Anna landed a few kilometres away and asks. Bert's position comes
        // within half a minute of the switch.
        var asked = 0
        var call: [String: Any] = [:]
        for _ in 0..<20 where asked == 0 {
            sleep(2)
            // A call nobody could be asked of is closed and asked again.
            if let id = call["id"] as? String {
                try await self.call(server, "DELETE", "/v1/help/\(id)", token: anna.token)
            }
            let answer = try await self.call(
                server, "POST", "/v1/help", token: anna.token,
                body: ["position": ["lat": 47.29, "lon": 11.42], "note": "Landed by the Inn, anyone driving to Innsbruck?"]
            )
            XCTAssertEqual(answer.status, 201)
            call = answer.json["request"] as? [String: Any] ?? [:]
            asked = call["asked"] as? Int ?? 0
        }
        XCTAssertEqual(asked, 1, "Bert was not asked: the app told no position.")
        let annasCall = try XCTUnwrap(call["id"] as? String)

        // In Community, first: who asks, the note, how far.
        openTab("Community", in: app)
        let row = element("community-help-request", in: app)
        for _ in 0..<6 where !row.exists {
            app.swipeDown()
            usleep(1_500_000)
        }
        XCTAssertTrue(row.appears(timeout: 20))
        XCTAssertTrue(row.label.contains("Anna Caller"), row.label)
        usleep(400_000)
        keepScreenshot("help-1-community-asked")
        row.tap()
        XCTAssertTrue(element("help-emergency", in: app).appears(timeout: 15))
        XCTAssertTrue(element("help-emergency", in: app).label.contains("112"))
        let line = element("help-line", in: app)
        XCTAssertTrue(line.appears(timeout: 15))
        XCTAssertTrue(line.label.contains("km") || line.label.contains("kilometre"), line.label)
        XCTAssertFalse(element("help-where", in: app).exists)
        usleep(400_000)
        keepScreenshot("help-2-asked")

        // "I can help": where Anna is, and the way there.
        element("help-can", in: app).tap()
        XCTAssertTrue(element("help-where", in: app).appears(timeout: 15))
        XCTAssertTrue(element("help-directions", in: app).exists)
        XCTAssertEqual(line.label, "You said you can help")
        usleep(400_000)
        keepScreenshots(light: "help-3-can-help", dark: "help-4-can-help-dark")
        let seen = try await self.call(server, "GET", "/v1/help/\(annasCall)", token: anna.token)
        let helpers = (seen.json["request"] as? [String: Any])?["helpers"] as? [[String: Any]] ?? []
        XCTAssertEqual(helpers.first?["handle"] as? String, bert.handle)
        try await self.call(server, "DELETE", "/v1/help/\(annasCall)", token: anna.token)

        // Now Bert asks. Anna may be asked, and is near.
        try await self.call(
            server, "PUT", "/v1/account/nearby", token: anna.token,
            body: ["open_plans_near": NSNull(), "may_be_asked_for_help": true]
        )
        try await self.call(
            server, "PUT", "/v1/account/position", token: anna.token,
            body: ["position": ["lat": 47.29, "lon": 11.42]]
        )
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let entry = bring("community-help", into: app)
        entry.tap()
        let note = element("help-note", in: app)
        XCTAssertTrue(note.appears(timeout: 15))
        XCTAssertTrue(element("help-call", in: app).exists)
        XCTAssertTrue(element("help-place", in: app).appears(timeout: 30))
        usleep(400_000)
        keepScreenshots(light: "help-5-ask", dark: "help-6-ask-dark")
        note.tap()
        note.typeText("By the river at Völs, a lift to Innsbruck?")
        element("help-ask", in: app).tap()
        let mine = element("help-line", in: app)
        XCTAssertTrue(mine.appears(timeout: 20))
        XCTAssertEqual(mine.label, "Asked 1 pilot nearby")

        // Anna can help: her name stands on Bert's call.
        let list = try await self.call(server, "GET", "/v1/help", token: anna.token)
        let bertsCall = try XCTUnwrap(((list.json["asked"] as? [[String: Any]])?.first)?["id"] as? String)
        try await self.call(server, "PUT", "/v1/help/\(bertsCall)/answer", token: anna.token, body: ["can_help": true])
        // The call is read again every 15 s while it stands.
        let helper = element("help-helper", in: app)
        XCTAssertTrue(helper.appears(timeout: 40))
        XCTAssertTrue(helper.label.contains("Anna Caller"), helper.label)
        XCTAssertEqual(mine.label, "Asked 1 pilot nearby · 1 can help")
        usleep(400_000)
        keepScreenshots(light: "help-7-mine-answered", dark: "help-8-mine-answered-dark")

        // "I'm fine, close": it has ended.
        bring("help-close", into: app).tap()
        XCTAssertTrue(element("help-ended", in: app).appears(timeout: 15))
        app.resetAuthorizationStatus(for: .location)
    }
}

@MainActor
extension XCUIElement {
    /// Whether the element is there, or comes within `timeout` seconds. In
    /// place of XCTest's `waitForExistence(timeout:)`, which looks for the
    /// first time only a second after it is asked, even where the element
    /// is there already: the 1,929 waits of the suite's run of 2026-11-23
    /// spent half an hour on that second alone. This looks at once, then
    /// four times a second.
    func appears(timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while !exists {
            if Date() >= deadline {
                return false
            }
            Thread.sleep(forTimeInterval: 0.25)
        }
        return true
    }

    /// Whether the element is gone, or goes within `timeout` seconds: the
    /// same, in place of `waitForNonExistence(timeout:)`.
    func vanishes(timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while exists {
            if Date() >= deadline {
                return false
            }
            Thread.sleep(forTimeInterval: 0.25)
        }
        return true
    }
}
