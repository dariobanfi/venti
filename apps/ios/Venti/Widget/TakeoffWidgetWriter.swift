import BackgroundTasks
import Foundation
import VentiCore
import WidgetKit

/// Writes what the home-screen widget draws: the next days at each of the
/// pilot's own take-offs, as the core says them.
///
/// The widget links no Rust and asks no server, so the app does both for
/// it: whenever Today has fresh forecasts, when the app leaves the screen,
/// and a few times a day when the system wakes it. Which take-offs are the
/// pilot's own and in which order is the core's (`todayTakeoffs` without a
/// place: the pilot's own, by name); what a forecast says of a day is the
/// core's (`siteOutlook`), asked for now and for the beginning of the next
/// day, so that the widget has the day's names as Today will have them
/// after midnight (`TakeoffSnapshot.Edition`). This class holds no rule
/// and no word.
@MainActor
final class TakeoffWidgetWriter {
    /// The app's writer, for the background refresh and for the moment the
    /// app leaves the screen, which both start outside the models.
    static var running: TakeoffWidgetWriter?

    /// The server to ask. The app sets this.
    var server: () -> URL? = { nil }
    /// The moment Today's forecasts take for now: Venti's clock. The app
    /// sets this to Today's own. Where it is not known (0), the iPhone's.
    var clock: () async -> Int64 = { 0 }

    private let core: Core
    private let store: TakeoffSnapshotStore?
    private let transport: TodayModel.Transport
    private let device: @Sendable () -> Int64
    private let reload: @MainActor () -> Void
    private var isWriting = false

    /// `store` is where the snapshot goes; without one nothing is written.
    /// `device` is the iPhone's clock, by which the widget counts a
    /// snapshot's age.
    init(
        core: Core, store: TakeoffSnapshotStore?,
        transport: @escaping TodayModel.Transport = WindModel.fetch,
        device: @escaping @Sendable () -> Int64 = { Int64(Date().timeIntervalSince1970) },
        reload: @escaping @MainActor () -> Void = { WidgetCenter.shared.reloadTimelines(ofKind: TakeoffWidget.kind) }
    ) {
        self.core = core
        self.store = store
        self.transport = transport
        self.device = device
        self.reload = reload
    }

    /// Reads the forecast of each of the pilot's own take-offs and writes
    /// the snapshot. A take-off whose forecast cannot be read keeps what
    /// was last read of it, with the time of that reading. `onlyIfDue`
    /// leaves a snapshot alone that has the same take-offs and was read
    /// within the time Today keeps an answer.
    func bringUpToDate(onlyIfDue: Bool = false) async {
        guard let store, !isWriting else { return }
        isWriting = true
        defer { isWriting = false }
        let core = self.core
        guard let listed = try? await Task.detached(operation: { try core.todayTakeoffs(place: nil, near: nil) }).value
        else { return }
        let sites = listed.takeoffs.filter(\.isKept).map(\.site)
        let before = store.read()
        let written = device()
        if onlyIfDue, let before, before.takeoffs.map(\.id) == sites.map(\.id),
           before.takeoffs.allSatisfy({ written - $0.updatedUnixS < Int64(TodayModel.keepFor) }) {
            return
        }
        var read: [String: [TakeoffSnapshot.Edition]] = [:]
        if let server = server() {
            let venti = await clock()
            let now = venti == 0 ? written : venti
            let transport = self.transport
            await withTaskGroup(of: (String, [TakeoffSnapshot.Edition]).self) { group in
                for site in sites {
                    group.addTask {
                        // Asked once: a forecast Venti has still to fetch
                        // is read the next time.
                        guard let answer = await TodayModel.forecastAnswer(
                            for: site, from: server, transport: transport, tries: 1, pause: .zero
                        ) else { return (site.id, []) }
                        return (site.id, Self.editions(of: site, in: answer, nowUnixS: now))
                    }
                }
                for await (id, editions) in group where !editions.isEmpty {
                    read[id] = editions
                }
            }
        }
        let snapshot = TakeoffSnapshot(takeoffs: sites.map { site in
            let kept = before?.takeoffs.first { $0.id == site.id }
            return TakeoffSnapshot.Takeoff(
                id: site.id, name: site.name,
                updatedUnixS: read[site.id] == nil ? kept?.updatedUnixS ?? 0 : written,
                editions: read[site.id] ?? kept?.editions ?? []
            )
        })
        guard snapshot != before else { return }
        guard (try? store.write(snapshot)) != nil else { return }
        reload()
    }

    /// The days as the core says them now, and as it says them from the
    /// beginning of the next day on. Empty where the answer has no days.
    nonisolated static func editions(
        of site: SiteView, in answer: (status: Int, body: Data), nowUnixS: Int64
    ) -> [TakeoffSnapshot.Edition] {
        var editions: [TakeoffSnapshot.Edition] = []
        var from = nowUnixS
        for _ in 0..<2 {
            guard let days = TodayModel.outlook(of: site, in: answer, nowUnixS: from)?.days, let first = days.first
            else { break }
            // The names hold until the next day begins.
            let until = days.first { $0.startUnixS > from }?.startUnixS ?? first.startUnixS + TakeoffSnapshot.goodForS
            editions.append(TakeoffSnapshot.Edition(fromUnixS: from, untilUnixS: until, days: days.map(TakeoffSnapshot.Day.init)))
            from = until
        }
        return editions
    }

    // MARK: In the background

    /// The identifier of the background refresh that brings the widget up
    /// to date, as `BGTaskSchedulerPermittedIdentifiers` in the Info.plist
    /// names it.
    nonisolated static var refreshTask: String {
        (Bundle.main.bundleIdentifier ?? "fly.venti") + ".widget-days"
    }

    /// How long after the last the system is asked to wake the app again:
    /// a few times a day, where it allows.
    static let refreshEvery: TimeInterval = 4 * 3600

    /// The app left the screen: the snapshot is brought up to date where
    /// it is due (a take-off kept or forgotten since, or an old reading),
    /// and the system is asked to wake the app later.
    func leftTheScreen() {
        guard !LaunchOptions.isUnderTest else { return }
        Task { await bringUpToDate(onlyIfDue: true) }
        scheduleRefresh()
    }

    /// Asks the system to wake the app in a few hours, while the pilot
    /// keeps a take-off for the widget to show.
    func scheduleRefresh() {
        guard !LaunchOptions.isUnderTest, store?.read()?.takeoffs.isEmpty == false else { return }
        let request = BGAppRefreshTaskRequest(identifier: Self.refreshTask)
        request.earliestBeginDate = Date(timeIntervalSinceNow: Self.refreshEvery)
        try? BGTaskScheduler.shared.submit(request)
    }

    /// What the background refresh does: the snapshot anew, and the next
    /// refresh.
    static func refreshInTheBackground() async {
        guard let running else { return }
        await running.bringUpToDate()
        running.scheduleRefresh()
    }
}

extension TakeoffSnapshot.Day {
    /// A day of the core's outlook, in the core's words.
    init(_ day: OutlookDay) {
        let verdict: TakeoffSnapshot.Verdict =
            switch day.verdict {
            case .flyable: .flyable
            case .marginal: .marginal
            case .notFlyable: .notFlyable
            }
        self.init(startUnixS: day.startUnixS, name: day.name, verdict: verdict, status: day.status, line: day.headline)
    }
}
