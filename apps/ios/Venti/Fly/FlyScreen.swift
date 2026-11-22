import SwiftUI
import VentiCore

/// The Fly tab: what to check before take-off, and the way into the cockpit.
struct FlyScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AppNavigation.self) private var navigation
    @Environment(FlySession.self) private var fly
    @Environment(WaypointsModel.self) private var waypoints
    @Environment(AirspaceModel.self) private var airspace
    @Environment(TaskModel.self) private var tasks
    @Environment(AccountModel.self) private var account
    @Environment(OfflineMapsModel.self) private var maps

    @AppStorage(Preferences.liveAudience) private var liveAudience = ""
    @State private var isChoosingVario = false

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Button {
                        isChoosingVario = true
                    } label: {
                        LabeledContent {
                            Text(varioSummary)
                        } label: {
                            Label("Vario", image: "icon-vario")
                        }
                    }
                    .foregroundStyle(.primary)
                    // What the cockpit will warn from, to be seen before
                    // take-off and not found out in the air.
                    NavigationLink {
                        AirspaceScreen()
                    } label: {
                        LabeledContent {
                            Text(airspace.inALine)
                        } label: {
                            Label("Airspace", image: "icon-airspace")
                        }
                    }
                    .accessibilityIdentifier("airspace-before-takeoff")
                    // The task of the day, or a route planned for the
                    // flight: the cockpit leads along it.
                    NavigationLink {
                        TaskScreen()
                    } label: {
                        LabeledContent {
                            Text(tasks.task?.inALine ?? "None")
                        } label: {
                            Label("Task", systemImage: "flag.checkered")
                        }
                    }
                    .accessibilityIdentifier("task-before-takeoff")
                    // Who may watch the flight while it is under way.
                    NavigationLink {
                        LiveScreen()
                    } label: {
                        LabeledContent {
                            Text(liveSummary)
                        } label: {
                            Label("Live tracking", image: "icon-live")
                        }
                    }
                    .accessibilityIdentifier("live-before-takeoff")
                    // Whether the cockpit has a map without a network.
                    NavigationLink {
                        OfflineMapsScreen()
                    } label: {
                        LabeledContent {
                            Text(maps.inALine)
                        } label: {
                            Label("Offline map", image: "icon-map")
                        }
                    }
                    .accessibilityIdentifier("map-before-takeoff")
                }

                Section {
                    Button {
                        fly.startFlight()
                    } label: {
                        Text("Open the cockpit")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.accent))
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                    .accessibilityIdentifier("open-cockpit")
                } footer: {
                    Text("The cockpit waits on the ground and records from the moment you take off, with the screen off too. Two minutes after you have landed, the flight is saved to the logbook.")
                }

                Section {
                    Menu {
                        Button("In real time") {
                            startReplay(speed: 1)
                        }
                        Button("20 times faster") {
                            startReplay(speed: 20)
                        }
                    } label: {
                        // The whole row opens the menu, not its words
                        // alone: on a wide iPhone a tap in the row's middle
                        // fell beside the words and opened nothing.
                        Text("Replay the sample flight")
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(.rect)
                    }
                } footer: {
                    Text("Plays a recorded flight through the cockpit, to try it on the ground. Nothing is saved.")
                }
            }
            .navigationTitle("Fly")
        }
        .onAppear {
            // A flight that ends by itself is kept as one the pilot ends.
            let logbook = logbook
            let navigation = navigation
            let fly = fly
            fly.savedByItself = { recording in
                Self.keep(recording, in: logbook, showingItWith: navigation, for: fly)
            }
        }
        // "Start flight" on a site's page: this tab came to the front
        // for it and opens the cockpit. As a task, which runs once the tab
        // is on screen: a cockpit asked for before that has nothing to
        // open over, and stays shut.
        .task(id: navigation.wantsCockpit) {
            if navigation.wantsCockpit {
                navigation.wantsCockpit = false
                fly.startFlight()
            }
        }
        .sheet(isPresented: $isChoosingVario) {
            VarioSheet(vario: fly.vario)
                .presentationDetents([.medium, .large])
        }
        .fullScreenCover(
            isPresented: Binding(
                get: { fly.mode != nil },
                set: { _ in }
            )
        ) {
            CockpitScreen {
                finish()
            }
            .environment(fly)
            .environment(waypoints)
            .environment(airspace)
            .environment(tasks)
            .environment(maps)
        }
        // Outside the cockpit only: while it is open, or opening, it says
        // what went wrong among its own notices.
        .alert(
            "Flying",
            isPresented: Binding(
                get: { fly.problem != nil && fly.mode == nil },
                set: { isPresented in
                    if !isPresented {
                        fly.problem = nil
                    }
                }
            ),
            presenting: fly.problem
        ) { _ in
            Button("OK", role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
    }

    private var varioSummary: String {
        switch fly.vario.state {
        case .connected(let name): name
        case .connecting(let name): "Looking for \(name)"
        case .unavailable: "Bluetooth unavailable"
        case .idle, .scanning: fly.vario.hasRememberedVario ? "Not in range" : "Phone"
        }
    }

    /// Who will watch the next flight. A choice that cannot be kept,
    /// because this iPhone is signed in nowhere, is not shown as if it
    /// could.
    private var liveSummary: String {
        let choice = LiveChoice(rawValue: liveAudience) ?? .nobody
        return choice == .nobody || account.isSignedIn ? choice.short : "Not signed in"
    }

    private func startReplay(speed: Double) {
        guard let url = Bundle.main.url(forResource: "sample", withExtension: "igc"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else {
            fly.problem = "The sample flight is missing from the app."
            return
        }
        fly.startReplay(igcText: text, speed: speed)
    }

    /// Ends the session, and for a real flight saves it and shows it.
    private func finish() {
        guard let recording = fly.finish() else { return }
        Self.keep(recording, in: logbook, showingItWith: navigation, for: fly)
    }

    /// Puts a recording into the logbook and shows the flight.
    private static func keep(
        _ recording: PendingRecording, in logbook: LogbookModel, showingItWith navigation: AppNavigation,
        for fly: FlySession
    ) {
        Task {
            do {
                navigation.show(flight: try await logbook.adopt(recording))
            } catch {
                fly.problem = "The flight was recorded but could not be added to the logbook: \(error.words) It will be tried again the next time Venti starts."
            }
        }
    }
}

/// The list of Bluetooth devices to pick a vario from.
private struct VarioSheet: View {
    let vario: VarioLink
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VentiList {
                if case .unavailable(let reason) = vario.state {
                    Section {
                        Label(reason, systemImage: "exclamationmark.triangle")
                    }
                }
                if let name = connectedOrWanted {
                    Section("Your vario") {
                        LabeledContent(name, value: vario.connectedName == nil ? "Looking for it" : "Connected")
                        Button("Forget this vario", role: .destructive) {
                            vario.forget()
                            vario.scan()
                        }
                    }
                }
                if vario.connectedName != nil {
                    Section {
                        LabeledContent("Received", value: "\(Format.count(vario.report.bytes)) bytes")
                        ForEach(Array(vario.report.lines.enumerated()), id: \.offset) { line in
                            Text(line.element)
                                .font(.venti(.caption))
                                .lineLimit(1)
                        }
                        ShareLink("Share a report", item: vario.reportText)
                    } header: {
                        Text("What it sends")
                    } footer: {
                        Text("If the cockpit shows no climb from this vario, share the report with the project: it says what the device offers and sends.")
                    }
                }
                Section {
                    ForEach(vario.devices) { device in
                        Button {
                            vario.connect(device)
                        } label: {
                            LabeledContent {
                                if device.looksLikeAVario {
                                    Text("Vario")
                                }
                            } label: {
                                Text(device.name)
                            }
                        }
                        .foregroundStyle(.primary)
                    }
                } header: {
                    HStack {
                        Text("Nearby devices")
                        if vario.state == .scanning {
                            ProgressView()
                        }
                    }
                } footer: {
                    Text("Without an external vario the phone's own pressure sensor is used. It is slower. Switch the vario on and choose it here, not in the iPhone's Bluetooth settings. Venti reads LK8EX1, OpenVario, XC Tracer, LXWP0 and BlueFly data.")
                }
            }
            .navigationTitle("Vario")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
        .onAppear {
            if vario.connectedName == nil {
                vario.scan()
            }
        }
        .onDisappear {
            vario.stopScan()
        }
    }

    private var connectedOrWanted: String? {
        switch vario.state {
        case .connected(let name), .connecting(let name): name
        default: nil
        }
    }
}
