import SwiftUI
import VentiCore
import XCTest

@testable import Venti

/// The home-screen widget "the next days at my take-off": what the app
/// writes for it, what the widget reads in that at a later moment, and its
/// face, kept as pictures.
///
/// The widget itself runs in the home screen's process, which no test here
/// reaches: its face (`TakeoffDaysView`, the same view the widget draws) is
/// drawn in the unit tests' host and kept as PNGs in the result bundle.
@MainActor
final class TakeoffWidgetTests: XCTestCase {
    // MARK: What is read in a snapshot

    /// Saturday 4 July 2026 from midnight UTC, its morning at 09:00, and
    /// the days after it.
    private let saturday: Int64 = 1_783_123_200
    private var morning: Int64 { saturday + 9 * 3600 }
    private var sunday: Int64 { saturday + 86_400 }
    private var monday: Int64 { saturday + 2 * 86_400 }

    private func day(_ start: Int64, _ name: String, _ verdict: TakeoffSnapshot.Verdict, _ line: String? = nil)
        -> TakeoffSnapshot.Day
    {
        let status =
            switch verdict {
            case .flyable: "Flyable"
            case .marginal: "Marginal"
            case .notFlyable: "Not flyable"
            }
        return TakeoffSnapshot.Day(startUnixS: start, name: name, verdict: verdict, status: status, line: line ?? status)
    }

    /// The Wallberg read on Saturday morning: the days as Today names
    /// them then, and as it names them from Sunday's first moment on.
    private func wallberg(editions: Int = 2) -> TakeoffSnapshot.Takeoff {
        let now = TakeoffSnapshot.Edition(fromUnixS: morning, untilUnixS: sunday, days: [
            day(saturday, "Today", .flyable, "Good from noon"),
            day(sunday, "Tomorrow", .marginal),
            day(monday, "Mon", .notFlyable),
        ])
        let later = TakeoffSnapshot.Edition(fromUnixS: sunday, untilUnixS: monday, days: [
            day(sunday, "Today", .marginal),
            day(monday, "Tomorrow", .notFlyable),
        ])
        return TakeoffSnapshot.Takeoff(
            id: "site:wallberg", name: "Wallberg", updatedUnixS: morning, editions: Array([now, later].prefix(editions))
        )
    }

    func testASnapshotIsWrittenAndReadAsItWas() throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-widget-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: folder) }
        let store = TakeoffSnapshotStore(file: folder.appendingPathComponent("takeoff-days.json"))

        XCTAssertNil(store.read(), "Nothing is read where nothing was written")
        let snapshot = TakeoffSnapshot(takeoffs: [wallberg()])
        try store.write(snapshot)

        XCTAssertEqual(store.read(), snapshot)
        XCTAssertEqual(
            store.read()?.shown(takeoff: nil, nowUnixS: morning + 60),
            .days(name: "Wallberg", days: wallberg().editions[0].days, updatedUnixS: morning)
        )
    }

    func testASnapshotOlderThanADaySaysSoInPlaceOfTheDays() {
        let snapshot = TakeoffSnapshot(takeoffs: [wallberg()])

        // Just under a day old it still shows days: Sunday's, by then.
        guard case .days = snapshot.shown(takeoff: nil, nowUnixS: morning + 86_400 - 1) else {
            return XCTFail("A snapshot under a day old shows its days")
        }
        XCTAssertEqual(snapshot.shown(takeoff: nil, nowUnixS: morning + 86_400), .outOfDate(name: "Wallberg"))
        XCTAssertEqual(snapshot.shown(takeoff: nil, nowUnixS: morning + 30 * 86_400), .outOfDate(name: "Wallberg"))
    }

    func testADayThatHasPassedIsDroppedAndTheNextIsToday() {
        let snapshot = TakeoffSnapshot(takeoffs: [wallberg()])

        // After midnight Saturday is gone, and Sunday is called "Today",
        // in the core's own words for that moment.
        guard case .days(_, let days, _) = snapshot.shown(takeoff: nil, nowUnixS: sunday + 6 * 3600) else {
            return XCTFail("Sunday morning shows Sunday's days")
        }
        XCTAssertEqual(days.map(\.startUnixS), [sunday, monday])
        XCTAssertEqual(days.map(\.name), ["Today", "Tomorrow"])
        XCTAssertEqual(days.first?.verdict, .marginal)
    }

    func testADayThatHasPassedIsNeverShownUnderItsOldName() {
        // Only Saturday's names were written: on Sunday nothing of them is
        // shown, not Saturday as "Today" and not Sunday as "Tomorrow".
        let snapshot = TakeoffSnapshot(takeoffs: [wallberg(editions: 1)])

        guard case .days = snapshot.shown(takeoff: nil, nowUnixS: sunday - 1) else {
            return XCTFail("Saturday's last second still shows Saturday's days")
        }
        XCTAssertEqual(snapshot.shown(takeoff: nil, nowUnixS: sunday), .outOfDate(name: "Wallberg"))
    }

    func testWithoutATakeoffTheWidgetSaysToKeepOne() {
        XCTAssertEqual(TakeoffSnapshot().shown(takeoff: nil, nowUnixS: morning), .noTakeoff)
        XCTAssertEqual(TakeoffSnapshot().shown(takeoff: "site:gone", nowUnixS: morning), .noTakeoff)
        XCTAssertEqual(TakeoffSnapshot().changes(takeoff: nil, after: morning), [])
    }

    func testTheFirstTakeoffIsShownUntilAnotherIsChosen() {
        var brauneck = wallberg()
        brauneck.id = "site:brauneck"
        brauneck.name = "Brauneck"
        brauneck.editions = []
        brauneck.updatedUnixS = 0
        let snapshot = TakeoffSnapshot(takeoffs: [wallberg(), brauneck])

        guard case .days(let name, _, _) = snapshot.shown(takeoff: nil, nowUnixS: morning) else {
            return XCTFail("The first take-off's days are shown")
        }
        XCTAssertEqual(name, "Wallberg")
        // The chosen one, which no forecast has been read for yet.
        XCTAssertEqual(snapshot.shown(takeoff: "site:brauneck", nowUnixS: morning), .noForecast(name: "Brauneck"))
        // One the pilot no longer keeps: the first again.
        guard case .days(let other, _, _) = snapshot.shown(takeoff: "site:gone", nowUnixS: morning) else {
            return XCTFail("A take-off that is gone gives way to the first")
        }
        XCTAssertEqual(other, "Wallberg")
    }

    func testTheWidgetIsDrawnAnewWhenTheDaysMoveOnAndWhenTheyAreADayOld() {
        let snapshot = TakeoffSnapshot(takeoffs: [wallberg()])

        XCTAssertEqual(snapshot.changes(takeoff: nil, after: morning), [sunday, morning + 86_400, monday])
        XCTAssertEqual(snapshot.changes(takeoff: nil, after: monday), [])
    }

    func testAnAddressOfTheWidgetIsToday() throws {
        XCTAssertTrue(TakeoffWidget.isTodaysAddress(try XCTUnwrap(TakeoffWidget.opensToday)))
        XCTAssertFalse(TakeoffWidget.isTodaysAddress(try XCTUnwrap(URL(string: "file:///tmp/flight.igc"))))
    }

    func testTheSimulatorBuildHasTheFolderTheWidgetReads() {
        // The app group of the entitlements, which a simulator grants
        // without a team or a profile.
        XCTAssertNotNil(TakeoffSnapshotStore.shared)
    }

    // MARK: What the app writes

    /// A Venti with one take-off and a forecast of calm hours for two
    /// days, in the server's own formats.
    private enum Answers {
        static let sites = """
        {"layers": [{
            "source": "osm-free-flying", "fetched_at": "2026-07-04T05:10:00Z", "as_of": "2026-07-03T21:21:02Z",
            "covers": [{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}],
            "sites": [
                {"id": "osm-free-flying:node/1", "name": "Wallberg Süd", "position": {"lat": 47.6652, "lon": 11.7954},
                 "elevation_m": 1620.0, "uses": ["takeoff"]}
            ]
        }],
        "attributions": [{
            "source": "osm-free-flying", "provider": "OpenStreetMap contributors", "licence": "ODbL 1.0",
            "text": "© OpenStreetMap contributors", "url": "https://www.openstreetmap.org/copyright"
        }]}
        """

        static var forecast: String {
            let hours = ["04", "05"].flatMap { day in
                (5...19).map { hour in
                    """
                    {"time": "2026-07-\(day)T\(String(format: "%02d", hour)):00:00Z", "wind_speed_ms": 2.0, \
                    "wind_gust_ms": 3.0, "wind_direction_deg": 180.0, "temperature_c": 20.0, \
                    "cloud_cover_pct": 10.0, "precipitation_mm": 0.0}
                    """
                }
            }
            return """
            {"point": {"lat": 47.6652, "lon": 11.7954}, "fetched_at": "2026-07-04T06:40:00Z",
             "layers": [{"source": "open-meteo", "series": [
               {"source": "open-meteo", "model": "icon-d2", "run": "2026-07-04T03:00:00Z",
                "point": {"lat": 47.6652, "lon": 11.7954}, "cell": {"lat": 47.66, "lon": 11.8},
                "cell_elevation_m": 1400.0,
                "hours": [\(hours.joined(separator: ","))]}
             ]}],
             "attributions": [{"source": "open-meteo", "provider": "Open-Meteo", "licence": "CC BY 4.0",
                               "text": "Weather data by Open-Meteo.com", "url": "https://open-meteo.com/"}]}
            """
        }
    }

    /// The app's models over a throwaway core and that Venti, with the
    /// widget's file in a throwaway folder. `reaches` says whether the
    /// forecast can be had.
    private func models(reaches: @escaping @Sendable () -> Bool = { true }) throws -> (
        today: TodayModel, own: KeptSitesModel, writer: TakeoffWidgetWriter, store: TakeoffSnapshotStore,
        reloads: () -> Int
    ) {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("venti-widget-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: folder) }
        let core = try Core(dataFolder: folder.appendingPathComponent("core", isDirectory: true).path)
        let transport: TodayModel.Transport = { url in
            if url.path.hasSuffix("/sites") {
                return (200, Data(Answers.sites.utf8))
            }
            return reaches() ? (200, Data(Answers.forecast.utf8)) : (0, Data())
        }
        let own = KeptSitesModel(core: core)
        let today = TodayModel(kept: own, transport: transport)
        let server = try XCTUnwrap(URL(string: "http://127.0.0.1:8080"))
        let now = morning
        today.server = { server }
        today.clock = { now }
        let store = TakeoffSnapshotStore(file: folder.appendingPathComponent("takeoff-days.json"))
        let counter = Counter()
        let writer = TakeoffWidgetWriter(
            core: core, store: store, transport: transport, device: { now }, reload: { counter.count += 1 }
        )
        writer.server = { server }
        writer.clock = today.clock
        return (today, own, writer, store, { counter.count })
    }

    @MainActor
    private final class Counter {
        var count = 0
    }

    func testTheAppWritesTheCoresWordsForThePilotsOwnTakeoffs() async throws {
        let (today, own, writer, store, reloads) = try models()

        // No take-off kept: an empty snapshot, which the widget answers
        // with its sentence.
        await writer.bringUpToDate()
        XCTAssertEqual(store.read(), TakeoffSnapshot())

        await today.load(around: Coordinate(lat: 47.6, lon: 11.75))
        let place = try XCTUnwrap(today.places.first)
        try await own.keep(place.site, credits: [], asOfUnixS: nil)
        await writer.bringUpToDate()

        let takeoff = try XCTUnwrap(store.read()?.takeoffs.first)
        XCTAssertEqual(store.read()?.takeoffs.count, 1)
        XCTAssertEqual(takeoff.name, place.site.name)
        XCTAssertEqual(takeoff.updatedUnixS, morning)
        XCTAssertEqual(reloads(), 2, "The widget is asked to draw anew after each writing")

        // Now: the days and their words as Today has them.
        let outlook = try XCTUnwrap(place.outlook)
        XCTAssertEqual(takeoff.editions.count, 2)
        XCTAssertEqual(takeoff.editions[0].fromUnixS, morning)
        XCTAssertEqual(takeoff.editions[0].days.map(\.name), outlook.days.map(\.name))
        XCTAssertEqual(takeoff.editions[0].days.map(\.status), outlook.days.map(\.status))
        XCTAssertEqual(takeoff.editions[0].days.map(\.line), outlook.days.map(\.headline))
        XCTAssertEqual(takeoff.editions[0].days.first?.name, "Today")

        // From the next day's first moment on: that day is the first, and
        // the core calls it "Today".
        let next = try XCTUnwrap(outlook.days.dropFirst().first)
        XCTAssertEqual(takeoff.editions[0].untilUnixS, next.startUnixS)
        XCTAssertEqual(takeoff.editions[1].fromUnixS, next.startUnixS)
        XCTAssertEqual(takeoff.editions[1].days.first?.startUnixS, next.startUnixS)
        XCTAssertEqual(takeoff.editions[1].days.first?.name, "Today")
    }

    func testATakeoffWhoseForecastCannotBeReadKeepsWhatWasRead() async throws {
        let reaches = Reaches()
        let (today, own, writer, store, reloads) = try models(reaches: { reaches.value })
        await today.load(around: Coordinate(lat: 47.6, lon: 11.75))
        try await own.keep(try XCTUnwrap(today.places.first).site, credits: [], asOfUnixS: nil)
        await writer.bringUpToDate()
        let written = try XCTUnwrap(store.read())

        reaches.value = false
        await writer.bringUpToDate()

        XCTAssertEqual(store.read(), written)
        XCTAssertEqual(reloads(), 1, "Nothing new is nothing to draw anew")

        // And a snapshot that is not due is left alone without asking.
        reaches.value = true
        await writer.bringUpToDate(onlyIfDue: true)
        XCTAssertEqual(reloads(), 1)
    }

    private final class Reaches: @unchecked Sendable {
        private let lock = NSLock()
        private var held = true
        var value: Bool {
            get { lock.withLock { held } }
            set { lock.withLock { held = newValue } }
        }
    }

    func testTodayTellsWhenItsForecastsAreIn() async throws {
        let (today, _, _, _, _) = try models()
        var told = 0
        today.onLoaded = { told += 1 }

        await today.load(around: Coordinate(lat: 47.6, lon: 11.75))

        XCTAssertEqual(told, 1)
    }

    // MARK: The face, as pictures

    /// The widget's sizes on an iPhone 17 and, the smallest there is, on
    /// an iPhone SE.
    private static let sizes: [(name: String, size: TakeoffDaysView.Size, width: CGFloat, height: CGFloat)] = [
        ("small", .small, 158, 158), ("medium", .medium, 338, 158),
    ]

    private func keep(
        _ shown: TakeoffSnapshot.Shown, as name: String, size: TakeoffDaysView.Size, width: CGFloat, height: CGFloat,
        scheme: ColorScheme, text: DynamicTypeSize = .large
    ) throws {
        let face = TakeoffDaysView(shown: shown, size: size)
            .frame(width: width, height: height)
            .background(.venti(.surface))
            .clipShape(.rect(cornerRadius: VentiRadius.l, style: .continuous))
            .ventiThemed()
            .environment(\.colorScheme, scheme)
            .environment(\.dynamicTypeSize, text)
        let renderer = ImageRenderer(content: face)
        renderer.scale = 3
        let image = try XCTUnwrap(renderer.uiImage)
        let attachment = XCTAttachment(image: image)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testTheWidgetsFaceIsKeptAsPictures() throws {
        let four = [
            day(saturday, "Today", .flyable, "Good from 11:00 to 15:00"),
            day(sunday, "Tomorrow", .marginal),
            day(monday, "Mon", .notFlyable),
            day(monday + 86_400, "Tue", .flyable, "Good all day"),
        ]
        let states: [(String, TakeoffSnapshot.Shown)] = [
            ("days", .days(name: "Wallberg Süd", days: four, updatedUnixS: morning + 5 * 3600 + 300)),
            ("no-takeoff", .noTakeoff),
            ("out-of-date", .outOfDate(name: "Wallberg Süd")),
            ("no-forecast", .noForecast(name: "Wallberg Süd")),
        ]
        for (state, shown) in states {
            for (name, size, width, height) in Self.sizes {
                for (appearance, scheme) in [("light", ColorScheme.light), ("dark", .dark)] {
                    try keep(
                        shown, as: "widget-\(state)-\(name)-\(appearance)", size: size, width: width, height: height,
                        scheme: scheme
                    )
                }
            }
        }
        // The days where room is scarcest: the smallest iPhone's widgets,
        // and the text a third larger.
        let days = states[0].1
        try keep(days, as: "widget-days-small-narrowest-light", size: .small, width: 148, height: 148, scheme: .light)
        try keep(days, as: "widget-days-medium-narrowest-light", size: .medium, width: 321, height: 148, scheme: .light)
        try keep(
            days, as: "widget-days-small-larger-text-light", size: .small, width: 158, height: 158, scheme: .light,
            text: .xxxLarge
        )
        try keep(
            days, as: "widget-days-medium-larger-text-dark", size: .medium, width: 338, height: 158, scheme: .dark,
            text: .xxxLarge
        )
    }
}
