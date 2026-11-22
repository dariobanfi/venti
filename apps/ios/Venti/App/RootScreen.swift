import SwiftUI

/// The app's sections, as the design language's tab bar has them. The
/// assistant and the settings are not among them: they open from the
/// Today tab's corner.
enum AppTab: Hashable {
    case today
    case map
    case fly
    case logbook
    case league
}

/// A flight to show, as a step in a navigation stack.
struct FlightRoute: Hashable {
    let id: String
}

/// The statistics screen, as a step in the logbook's navigation stack.
struct StatisticsRoute: Hashable {}

/// Where the pilot is in the app. Held in one place so that a file opened
/// from outside can bring its flight to the front.
@MainActor
@Observable
final class AppNavigation {
    var tab: AppTab = .today
    var logbookPath = NavigationPath()
    /// Set when the cockpit was asked for from another tab, a site's
    /// page for one: the Fly tab opens it and takes this back.
    var wantsCockpit = false

    /// "Start flight" outside the Fly tab: that tab comes to the front
    /// and opens the cockpit, which waits for the take-off.
    func startFlight() {
        tab = .fly
        wantsCockpit = true
    }

    func show(flight id: String) {
        tab = .logbook
        logbookPath = NavigationPath([FlightRoute(id: id)])
    }
}

struct RootScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AccountModel.self) private var account
    @Environment(AlertsModel.self) private var alerts
    @Environment(TaskModel.self) private var tasks
    @Environment(\.scenePhase) private var scenePhase
    @State private var navigation = AppNavigation()
    @State private var notice: ImportNotice?

    var body: some View {
        @Bindable var navigation = navigation
        TabView(selection: $navigation.tab) {
            // The icons are the design pack's.
            Tab("Today", image: "icon-forecast", value: AppTab.today) {
                TodayScreen()
            }
            Tab("Map", image: "icon-map", value: AppTab.map) {
                MapScreen()
            }
            Tab("Fly", image: "icon-wing", value: AppTab.fly) {
                FlyScreen()
            }
            Tab("Logbook", image: "icon-track", value: AppTab.logbook) {
                LogbookScreen()
            }
            Tab("League", image: "icon-league", value: AppTab.league) {
                LeagueScreen()
            }
        }
        .environment(navigation)
        // A flight that was being recorded when the app last stopped.
        .task {
            await logbook.adoptInterruptedRecordings()
        }
        // With an account, the logbook is brought in step with the server
        // when the app comes to the front and whenever the logbook has
        // changed: after an import, a deletion or a flight.
        .onChange(of: scenePhase, initial: true) { _, phase in
            if phase == .active {
                Task { await account.sync() }
                // And what Venti says of the account: whether it is a
                // partner for flying sites, which decides what a site's
                // page offers.
                Task { await account.read() }
                // And what the alarms fired since, each new line a
                // notification: Venti sends nothing by itself.
                Task { await alerts.sync(notifies: true) }
            } else if phase == .background {
                alerts.scheduleRefresh()
                // The home-screen widget, which the pilot may look at next.
                TakeoffWidgetWriter.running?.leftTheScreen()
            }
        }
        .onChange(of: logbook.view.totals.flights) {
            Task { await account.sync(force: true) }
        }
        // A file opened from Files, Mail or another app: a task, which is
        // kept and found on the Fly tab, or a flight. Or the widget's tap.
        .onOpenURL { url in
            // A tap on the home-screen widget: Today.
            if TakeoffWidget.isTodaysAddress(url) {
                navigation.tab = .today
                return
            }
            if url.pathExtension.lowercased() == "xctsk" {
                let isKept = tasks.keep(fileAt: url)
                Self.removeIfInInbox(url)
                if isKept {
                    navigation.tab = .fly
                    notice = ImportNotice(
                        title: "Task kept",
                        text: "It replaces the task you had. The cockpit flies it; look at it on the Fly tab, under Task."
                    )
                } else {
                    notice = ImportNotice(title: "The task could not be read", text: tasks.problem ?? "")
                }
                return
            }
            Task {
                let report = await logbook.importFiles(at: [url])
                Self.removeIfInInbox(url)
                if let id = report.added.first ?? report.alreadyThere.first {
                    navigation.show(flight: id)
                } else {
                    notice = report.notice
                }
            }
        }
        .importNotice($notice)
    }

    /// The system hands a file opened from another app over as a copy in the
    /// app's inbox folder. Once imported the copy is of no use. Anything
    /// outside that folder is the pilot's own file and is never touched.
    private static func removeIfInInbox(_ url: URL) {
        guard let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
        else { return }
        let inbox = documents.appendingPathComponent("Inbox", isDirectory: true).standardizedFileURL.path
        if url.standardizedFileURL.path.hasPrefix(inbox + "/") {
            try? FileManager.default.removeItem(at: url)
        }
    }
}
