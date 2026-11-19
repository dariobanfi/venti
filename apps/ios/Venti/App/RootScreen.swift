import SwiftUI

/// The app's sections. More join as the product grows: the cockpit, then the
/// community.
enum AppTab: Hashable {
    case map
    case fly
    case logbook
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
            Tab("Settings", systemImage: "gearshape", value: AppTab.settings) {
                SettingsScreen()
            }
        }
        .environment(navigation)
        // A flight that was being recorded when the app last stopped.
        .task {
            await logbook.adoptInterruptedRecordings()
        }
        // A flight file opened from Files, Mail or another app.
        .onOpenURL { url in
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
