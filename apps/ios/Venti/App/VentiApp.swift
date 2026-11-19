import SwiftUI

@main
struct VentiApp: App {
    @State private var session = Session.start()

    var body: some Scene {
        WindowGroup {
            switch session {
            case .ready(let logbook, let fly):
                RootScreen()
                    .environment(logbook)
                    .environment(fly)
                    .tint(Palette.accent)
                    .preferredColorScheme(LaunchOptions.forcedColorScheme)
            case .failed(let message):
                ContentUnavailableView(
                    "Venti could not open its data",
                    systemImage: "externaldrive.badge.exclamationmark",
                    description: Text(message)
                )
            }
        }
    }
}

/// What the app holds for as long as it runs.
@MainActor
enum Session {
    case ready(LogbookModel, FlySession)
    case failed(String)

    static func start() -> Session {
        do {
            let folder = try LaunchOptions.dataFolder()
            return .ready(try LogbookModel.open(folder: folder), FlySession(dataFolder: folder))
        } catch {
            return .failed(error.localizedDescription)
        }
    }
}

/// Switches set on the command line. The UI tests use them; nothing in the
/// app offers them to the pilot.
enum LaunchOptions {
    private static let arguments = ProcessInfo.processInfo.arguments

    /// Start with an empty logbook in a throwaway folder.
    static let isUITesting = arguments.contains("--ui-testing")

    /// Whether the app is running under any test. Tests have no sound
    /// device to rely on and nobody listening.
    static let isUnderTest =
        isUITesting || ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil

    /// Draw the map from the region bundled with the app instead of fetching
    /// tiles, so a screenshot does not depend on the network.
    static let usesBundledMap = arguments.contains("--bundled-map")

    /// Dark appearance whatever the device is set to, for screenshots.
    /// Normally the app follows the device.
    static var forcedColorScheme: ColorScheme? {
        arguments.contains("--dark") ? .dark : nil
    }

    /// The folder the core keeps the pilot's flights in.
    static func dataFolder() throws -> URL {
        if isUITesting {
            return FileManager.default.temporaryDirectory
                .appendingPathComponent("venti-ui-test-\(UUID().uuidString)", isDirectory: true)
        }
        return try FileManager.default
            .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            .appendingPathComponent("Venti", isDirectory: true)
    }
}
