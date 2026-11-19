import SwiftUI

/// The app's sections. More join as the product grows: the cockpit, then the
/// community.
enum AppTab: Hashable {
    case map
    case fly
    case logbook
    case assistant
    case settings
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
    var tab: AppTab = .map
    var logbookPath = NavigationPath()

    func show(flight id: String) {
        tab = .logbook
        logbookPath = NavigationPath([FlightRoute(id: id)])
    }
}

struct RootScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AccountModel.self) private var account
    @Environment(TaskModel.self) private var tasks
    @Environment(\.scenePhase) private var scenePhase
    @State private var navigation = AppNavigation()
    @State private var notice: ImportNotice?

    var body: some View {
        @Bindable var navigation = navigation
        TabView(selection: $navigation.tab) {
            Tab("Map", systemImage: "map", value: AppTab.map) {
                MapScreen()
            }
            Tab("Fly", systemImage: "paperplane", value: AppTab.fly) {
                FlyScreen()
            }
            Tab("Logbook", systemImage: "book.closed", value: AppTab.logbook) {
                LogbookScreen()
            }
            Tab("Assistant", systemImage: "sparkles", value: AppTab.assistant) {
                AssistantScreen()
            }
            Tab("Settings", systemImage: "gearshape", value: AppTab.settings) {
                SettingsScreen()
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
            }
        }
        .onChange(of: logbook.view.totals.flights) {
            Task { await account.sync(force: true) }
        }
        // A file opened from Files, Mail or another app: a task, which is
        // kept and found on the Fly tab, or a flight.
        .onOpenURL { url in
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
