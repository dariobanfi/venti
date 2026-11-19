import SwiftUI
import VentiCore

@main
struct VentiApp: App {
    @State private var session = Session.start()

    var body: some Scene {
        WindowGroup {
            switch session {
            case .ready(let logbook, let fly, let chat, let wind, let waypoints, let airspace, let account, let sites):
                RootScreen()
                    .environment(logbook)
                    .environment(fly)
                    .environment(chat)
                    .environment(wind)
                    .environment(waypoints)
                    .environment(airspace)
                    .environment(account)
                    .environment(sites)
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
    case ready(
        LogbookModel, FlySession, AssistantChat, WindModel, WaypointsModel, AirspaceModel, AccountModel, SitesModel
    )
    case failed(String)

    static func start() -> Session {
        do {
            let folder = try LaunchOptions.dataFolder()
            let core = try Core(dataFolder: folder.path)
            let logbook = LogbookModel(core: core, folder: folder)
            if LaunchOptions.usesSampleWaypoints {
                _ = try? core.importWaypoints(text: LaunchOptions.sampleWaypoints)
            }
            let waypoints = WaypointsModel(core: core)
            if LaunchOptions.usesSampleAirspace {
                _ = try? core.importAirspace(
                    fileName: "Sample zones (not real).txt",
                    bytes: Data(LaunchOptions.sampleAirspace.utf8),
                    nowUnixS: Int64(Date().timeIntervalSince1970)
                )
            }
            // Under test the server is the one named on the command line,
            // or none: a test must not depend on what a pilot has saved.
            let wind =
                if LaunchOptions.isUnderTest {
                    WindModel(address: LaunchOptions.server ?? "")
                } else {
                    WindModel()
                }
            let airspace = AirspaceModel(core: core, remembersChoice: !LaunchOptions.isUnderTest)
            airspace.server = { wind.server }
            // Flying sites come from the same server as the wind.
            let sites = SitesModel()
            sites.server = { wind.server }
            // The assistant looks up live wind on the same server as the
            // map, and measures ages against the same clock.
            let scripted = LaunchOptions.usesScriptedAssistant
            let transport: AssistantChat.Transport = scripted ? ScriptedProvider.answer : AssistantChat.send
            let readKey: () -> String? = scripted ? { "scripted" } : { KeyStore.read() }
            let chat = AssistantChat(
                makeAssistant: { model in logbook.makeAssistant(model: model) },
                transport: transport,
                readKey: readKey,
                server: { wind.server?.absoluteString },
                now: { wind.now }
            )
            let fly = FlySession(dataFolder: folder)
            fly.airspace = { airspace.set }
            // A test starts signed out, whatever an earlier one left in
            // the simulator's Keychain.
            if LaunchOptions.isUITesting {
                SessionStore.remove()
            }
            let account = AccountModel(core: core)
            account.onLogbookChanged = { logbook.refresh() }
            return .ready(logbook, fly, chat, wind, waypoints, airspace, account, sites)
        } catch {
            return .failed(error.words)
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

    /// Answer the assistant's requests from a script instead of a provider,
    /// so its screen can be tested without a key or a network.
    static let usesScriptedAssistant = arguments.contains("--scripted-assistant")

    /// Start with a few made-up waypoints around the sample flight, so the
    /// cockpit's navigation can be tested without picking a file.
    static let usesSampleWaypoints = arguments.contains("--sample-waypoints")

    /// The made-up waypoints. Their names say that they are not real
    /// places to land.
    static let sampleWaypoints = """
        name,code,country,lat,lon,elev,style
        "Sample landing field",SLF,DE,4741.000N,01145.300E,735m,21
        "Sample turnpoint",STP,DE,4737.200N,01152.000E,1700m,1
        "Sample summit far away",SSF,DE,4725.260N,01059.090E,2962m,7
        """

    /// Start with a made-up control zone near the end of the sample
    /// flight, so the cockpit's airspace warnings can be tested.
    static let usesSampleAirspace = arguments.contains("--sample-airspace")

    /// The made-up airspace, in OpenAir format. Its names say that it is
    /// not real.
    static let sampleAirspace = """
        * Made-up airspace for testing Venti. Not real.
        AC D
        AY CTR
        AN SAMPLE ZONE (NOT REAL)
        AL GND
        AH 9000ft AMSL
        V X=47:42:00 N 011:44:00 E
        DC 1
        """

    /// The Venti server to use, given as `--server <address>`: the UI tests
    /// run one on the same machine.
    static var server: String? {
        guard let flag = arguments.firstIndex(of: "--server"), arguments.indices.contains(flag + 1) else {
            return nil
        }
        return arguments[flag + 1]
    }

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
