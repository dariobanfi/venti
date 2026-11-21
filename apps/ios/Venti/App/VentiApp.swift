import SwiftUI
import VentiCore

@main
struct VentiApp: App {
    @State private var session = Session.start()

    init() {
        // The bars are UIKit's: their type and ink are set once, before
        // the first of them is drawn.
        VentiAppearance.apply()
    }

    var body: some Scene {
        WindowGroup {
            switch session {
            case .ready(
                let logbook, let fly, let chat, let wind, let waypoints, let airspace, let account, let sites, let tasks,
                let live, let community, let ground, let groups, let maps, let today, let thermals
            ):
                RootScreen()
                    .environment(logbook)
                    .environment(fly)
                    .environment(chat)
                    .environment(wind)
                    .environment(waypoints)
                    .environment(airspace)
                    .environment(account)
                    .environment(sites)
                    .environment(tasks)
                    .environment(live)
                    .environment(community)
                    .environment(ground)
                    .environment(groups)
                    .environment(maps)
                    .environment(today)
                    .environment(thermals)
                    .ventiThemed()
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
        LogbookModel, FlySession, AssistantChat, WindModel, WaypointsModel, AirspaceModel, AccountModel, SitesModel,
        TaskModel, LiveModel, CommunityModel, TerrainModel, GroupsModel, OfflineMapsModel, TodayModel,
        ThermalMapModel
    )
    case failed(String)

    static func start() -> Session {
        do {
            let folder = try LaunchOptions.dataFolder()
            let core = try Core(dataFolder: folder.path)
            if LaunchOptions.usesSampleLogbook {
                for text in LaunchOptions.sampleLogbook() {
                    _ = try? core.importFlight(igcText: text)
                }
            }
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
            // And the forecasts for the take-offs of the Today tab.
            let today = TodayModel()
            today.server = { wind.server }
            today.clock = { await wind.serverNow() }
            // And the forecast map of the map tab.
            let thermals = ThermalMapModel()
            thermals.server = { wind.server }
            thermals.clock = { await wind.serverNow() }
            // So do the other pilots in the air.
            let live = LiveModel()
            live.server = { wind.server }
            // And what the pilots there share with everyone.
            let community = CommunityModel()
            community.server = { wind.server }
            // The assistant looks up live wind on the same server as the
            // map, and measures ages against the same clock.
            let scripted = LaunchOptions.usesScriptedAssistant
            let transport: AssistantChat.Transport = scripted ? ScriptedProvider.answer : AssistantChat.send
            let readKey: () -> String? = scripted ? { "scripted" } : { KeyStore.read() }
            let chat = AssistantChat(
                makeAssistant: { model in logbook.makeAssistant(model: model) },
                makeAssistantAt: { address, model in try logbook.makeAssistant(at: address, model: model) },
                transport: transport,
                readKey: readKey,
                // A test starts at Anthropic, whatever a pilot has set.
                remembersService: !LaunchOptions.isUnderTest,
                server: { wind.server?.absoluteString },
                now: { wind.now }
            )
            if LaunchOptions.usesSampleTask {
                _ = try? core.keepTask(text: LaunchOptions.sampleTask)
            }
            let tasks = TaskModel(core: core)
            let fly = FlySession(dataFolder: folder)
            fly.airspace = { airspace.set }
            fly.taskFile = { tasks.file }
            fly.landings = { waypoints.all.filter { $0.kind == .landing } }
            // Other pilots in the air around the glider, from the same
            // server as the map tab's.
            fly.others = { place in await live.around(place) }
            // And the wind on the ground, from the server's wind stations.
            fly.windOnTheGround = { place, landing in await wind.stations(around: place, andAround: landing) }
            // The ground under the glider, from the map's elevation tiles:
            // brought as the glider moves, and kept on the iPhone.
            let ground = TerrainModel()
            fly.terrain = { ground.terrain }
            // Maps downloaded for flying without a network, from the same
            // server as the wind. The ground is read from their elevation
            // files wherever they reach, and the cockpit's map is drawn
            // from them wherever the glider is inside one. A UI test keeps
            // its downloads in its throwaway folder.
            let maps = OfflineMapsModel(
                folder: LaunchOptions.isUITesting
                    ? folder.appendingPathComponent("Maps", isDirectory: true)
                    : OfflineMapsModel.store
            )
            maps.server = { wind.server }
            maps.onKeptChanged = { archives in ground.use(archives: archives) }
            ground.use(archives: maps.terrainArchives)
            fly.wantTerrain = { place in
                maps.flying(at: place)
                Task {
                    await ground.load(around: place)
                }
            }
            // A test starts signed out, whatever an earlier one left in
            // the simulator's Keychain.
            if LaunchOptions.isUITesting {
                SessionStore.remove()
            }
            let account = AccountModel(core: core)
            account.onLogbookChanged = { logbook.refresh() }
            // A flight shared while it is under way goes to the pilot's
            // server, with the account's token.
            fly.server = { account.isSignedIn ? account.account?.server : nil }
            fly.carry = { request in await account.carry(request) }
            // Groups and their plans for a flying day, on the pilot's
            // server, with the account's token.
            let groups = GroupsModel(core: core, account: account)
            return .ready(
                logbook, fly, chat, wind, waypoints, airspace, account, sites, tasks, live, community, ground,
                groups, maps, today, thermals
            )
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

    /// Start with the sample flight five times on other days of July 2026,
    /// so that the logbook holds enough thermals for "How you fly" to say
    /// something. The web's browser test does the same.
    static let usesSampleLogbook = arguments.contains("--sample-logbook")

    /// The sample flight on the 1st, 2nd, 3rd, 5th and 6th of July 2026:
    /// its date rewritten, since the logbook takes a flight with the same
    /// positions on the same day for one it already has.
    static func sampleLogbook() -> [String] {
        guard let url = Bundle.main.url(forResource: "sample", withExtension: "igc"),
            let text = try? String(contentsOf: url, encoding: .utf8)
        else { return [] }
        return ["01", "02", "03", "05", "06"].map { day in
            text.replacingOccurrences(of: "HFDTEDATE:040726", with: "HFDTEDATE:\(day)0726")
        }
    }

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

    /// Start with a made-up task along the sample flight, so the cockpit's
    /// task can be tested without picking a file.
    static let usesSampleTask = arguments.contains("--sample-task")

    /// The made-up task, as a file in the XCTSK format: out of a start
    /// cylinder around the sample flight's take-off two minutes into it,
    /// to a turnpoint the flight passes, and to a goal it reaches. Its
    /// names say that it is not real.
    static let sampleTask = """
        {"taskType":"CLASSIC","version":1,"earthModel":"WGS84","turnpoints":[
        {"type":"TAKEOFF","radius":400,"waypoint":{"name":"Sample launch","lat":47.6642,"lon":11.7967,"altSmoothed":1620}},
        {"type":"SSS","radius":1000,"waypoint":{"name":"Sample start","lat":47.6642,"lon":11.7967,"altSmoothed":1620}},
        {"radius":400,"waypoint":{"name":"Sample turnpoint west","lat":47.6894,"lon":11.7506,"altSmoothed":800}},
        {"radius":400,"waypoint":{"name":"Sample goal","lat":47.6638,"lon":11.7432,"altSmoothed":760}}],
        "takeoff":{"timeOpen":"09:00:00Z","timeClose":"11:00:00Z"},
        "sss":{"type":"RACE","direction":"EXIT","timeGates":["10:32:00Z"]},
        "goal":{"type":"CYLINDER","deadline":"12:00:00Z"}}
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
