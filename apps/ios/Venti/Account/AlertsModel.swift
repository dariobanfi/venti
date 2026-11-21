import BackgroundTasks
import Foundation
import Observation
import UserNotifications
import VentiCore

/// The pilot's alarms on the account and what they fired, as the screens
/// see them.
///
/// The choices, the words, the check of a draft, the requests and what the
/// answers mean are the core's (`alerts.rs`). This class sends the requests
/// with the session, as the account's others go, runs the core's
/// `alertsSync` to its end and holds what the core keeps on the iPhone.
/// Venti sends nothing by itself: the app fetches what fired when it comes
/// to the front, after signing in and after an alarm was saved, and from
/// the system's background refresh; what is new is a local notification.
@MainActor
@Observable
final class AlertsModel {
    /// Sends a request with the session's token and returns the answer.
    typealias Send = @MainActor (ServerRequest) async throws -> AccountModel.Answer

    /// The account's alarms as the iPhone last fetched them, the oldest
    /// first.
    private(set) var alerts: [AlertView] = []
    /// What they fired, the newest first, as the core's lines.
    private(set) var fired: [FiredLine] = []
    /// Whether the core's sync is under way.
    private(set) var isSyncing = false
    /// Why the last sync stopped, in the core's words; `nil` after one
    /// that went through. Said on the list, never as an alert.
    private(set) var problem: String?

    /// The sixteen points of the compass, short, from the north.
    let points = alertPoints()

    @ObservationIgnored private let core: Core
    @ObservationIgnored private let send: Send
    @ObservationIgnored private let isSignedIn: @MainActor () -> Bool
    @ObservationIgnored private let now: () -> Int64
    /// Shows what fired that the iPhone had not seen.
    @ObservationIgnored private let notify: @MainActor ([FiredLine]) async -> Void
    /// Asks the pilot whether the app may notify, once.
    @ObservationIgnored private let askToNotify: @MainActor () async -> Void
    /// Set when a sync was asked for while one was under way.
    @ObservationIgnored private var isSyncDue = false

    init(
        core: Core,
        send: @escaping Send,
        isSignedIn: @escaping @MainActor () -> Bool,
        now: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970) },
        notify: @escaping @MainActor ([FiredLine]) async -> Void = AlertNotices.post,
        askToNotify: @escaping @MainActor () async -> Void = AlertNotices.askOnce
    ) {
        self.core = core
        self.send = send
        self.isSignedIn = isSignedIn
        self.now = now
        self.notify = notify
        self.askToNotify = askToNotify
        read()
    }

    /// With the app's account.
    convenience init(core: Core, account: AccountModel) {
        self.init(
            core: core,
            send: { try await account.answer(to: $0) },
            isSignedIn: { account.isSignedIn }
        )
    }

    /// How far the iPhone's clock is ahead of UTC, for the core's lines.
    nonisolated static var utcOffsetS: Int32 {
        Int32(clamping: TimeZone.current.secondsFromGMT())
    }

    /// Reads what the core keeps on the iPhone: nothing signed out.
    func read() {
        alerts = (try? core.alertsHeld()) ?? []
        fired = (try? core.alertsFiredHeld(nowUnixS: now(), utcOffsetS: Self.utcOffsetS)) ?? []
    }

    // MARK: Fetching

    /// Fetches the alarms and what they fired since the last run, if the
    /// pilot is signed in. With `notifies` each line that is new becomes a
    /// notification: when the app comes to the front and in the
    /// background, not after signing in, when everything Venti keeps is
    /// new to the iPhone. Returns the new lines. Quiet: the list says why
    /// a run stopped.
    @discardableResult
    func sync(notifies: Bool = false) async -> [FiredLine] {
        guard isSignedIn() else {
            read()
            return []
        }
        guard !isSyncing else {
            isSyncDue = true
            return []
        }
        isSyncing = true
        defer { isSyncing = false }
        var new: [FiredLine] = []
        repeat {
            isSyncDue = false
            new += await runSync()
        } while isSyncDue && isSignedIn()
        read()
        if notifies, !new.isEmpty {
            await notify(new)
        }
        return new
    }

    /// One run of the core's sync, to its end.
    private func runSync() async -> [FiredLine] {
        let run = core.alertsSync()
        var step = run.start()
        while case .send(let request) = step {
            do {
                let answer = try await send(request)
                step = run.receive(status: answer.status, body: answer.body, retryAfterS: answer.retryAfter)
            } catch {
                problem = AccountModel.words(for: error)
                return []
            }
        }
        switch step {
        case .finished:
            problem = nil
            return run.newLines(utcOffsetS: Self.utcOffsetS)
        case .failed(let message, _):
            problem = message
            return []
        case .send:
            return []
        }
    }

    // MARK: Changing

    /// What is wrong with a draft, by the rules Venti holds to; empty
    /// when it may be sent.
    func faults(of draft: AlertDraft) -> [AlertFaultView] {
        alertCheck(draft: draft)
    }

    /// Sets a new alarm, or changes the one with `id`. Returns `nil` when
    /// it was saved, else why not in the core's words. The first alarm
    /// the pilot sets asks whether the app may notify.
    func save(_ draft: AlertDraft, id: String?) async -> String? {
        do {
            let request = try id.map { try alertChangeRequest(id: $0, draft: draft) }
                ?? alertAddRequest(draft: draft)
            let answer = try await send(request)
            _ = try alertSaved(status: answer.status, body: answer.body)
        } catch {
            return AccountModel.words(for: error)
        }
        if id == nil {
            await askToNotify()
        }
        await sync()
        return nil
    }

    /// Switches an alarm on or off. Returns `nil` when it was done.
    func set(_ alert: AlertView, enabled: Bool) async -> String? {
        guard var draft = alert.draft else { return nil }
        draft.enabled = enabled
        return await save(draft, id: alert.id)
    }

    /// Removes an alarm. Returns `nil` when it is gone.
    func remove(_ id: String) async -> String? {
        do {
            let answer = try await send(alertRemoveRequest(id: id))
            // The core reads the answer: one gone already is gone.
            _ = try alertRemoved(status: answer.status, body: answer.body)
        } catch {
            return AccountModel.words(for: error)
        }
        await sync()
        return nil
    }

    // MARK: Drafts

    /// A wind alarm for a station, its name filled in.
    static func draft(for station: WindStation) -> AlertDraft {
        empty(kind: .wind, name: station.name, stationId: station.id, stationName: station.name)
    }

    /// A day alarm for a take-off: today and tomorrow, a cross-country
    /// day or better.
    static func draft(for site: SiteView) -> AlertDraft {
        var draft = empty(kind: .day, name: site.name)
        draft.lat = site.position.lat
        draft.lon = site.position.lon
        draft.siteId = site.id
        draft.placeName = site.name
        draft.daysAhead = Data([0, 1])
        return draft
    }

    private static func empty(
        kind: AlertKind, name: String, stationId: String = "", stationName: String = ""
    ) -> AlertDraft {
        AlertDraft(
            name: name, enabled: true, kind: kind, stationId: stationId, stationName: stationName,
            fromPoint: nil, toPoint: nil, minKmh: nil, maxKmh: nil, gustsMaxKmh: nil,
            fromHour: nil, toHour: nil, lat: 0, lon: 0, siteId: nil, placeName: "",
            daysAhead: Data(), atLeast: .crossCountry
        )
    }

    // MARK: In the background

    /// The identifier of the background refresh that fetches what fired,
    /// as `BGTaskSchedulerPermittedIdentifiers` in the Info.plist names it.
    nonisolated static var refreshTask: String {
        (Bundle.main.bundleIdentifier ?? "fly.venti") + ".alerts"
    }

    /// The app's model, for the background refresh, which the system
    /// starts outside the screens.
    static weak var running: AlertsModel?

    /// Asks the system to wake the app in half an hour or so, while the
    /// pilot has an alarm switched on. Called when the app goes to the
    /// background and from the refresh itself.
    func scheduleRefresh() {
        guard isSignedIn(), alerts.contains(where: \.enabled), !LaunchOptions.isUnderTest else { return }
        let request = BGAppRefreshTaskRequest(identifier: Self.refreshTask)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 30 * 60)
        try? BGTaskScheduler.shared.submit(request)
    }

    /// What the background refresh does: the sync with its notifications,
    /// and the next refresh.
    static func refreshInTheBackground() async {
        guard let running else { return }
        await running.sync(notifies: true)
        running.scheduleRefresh()
    }
}

/// The local notifications of what fired: no push, the app posts them
/// itself. They show also while the app is in front, since that is when
/// it fetches.
enum AlertNotices {
    /// Posts one notification a line: the alarm's name and the core's line.
    @MainActor
    static func post(_ lines: [FiredLine]) async {
        let centre = UNUserNotificationCenter.current()
        for line in lines {
            let content = UNMutableNotificationContent()
            content.title = line.title
            content.body = line.line
            content.sound = .default
            let request = UNNotificationRequest(
                identifier: "alert-\(line.alertId)-\(line.firedAtUnixS)", content: content, trigger: nil
            )
            try? await centre.add(request)
        }
    }

    /// Asks whether the app may notify, if it has not asked yet.
    @MainActor
    static func askOnce() async {
        let centre = UNUserNotificationCenter.current()
        guard await centre.notificationSettings().authorizationStatus == .notDetermined else { return }
        _ = try? await centre.requestAuthorization(options: [.alert, .sound])
    }

    /// Shows the notifications while the app is in front too. Set once,
    /// at the start; the centre holds its delegate weakly.
    static let showsInFront = InFront()

    final class InFront: NSObject, UNUserNotificationCenterDelegate, Sendable {
        func userNotificationCenter(
            _ center: UNUserNotificationCenter, willPresent notification: UNNotification
        ) async -> UNNotificationPresentationOptions {
            [.banner, .list, .sound]
        }
    }
}
