import SwiftUI
import VentiCore

/// What the Fly tab listed before take-off, behind the cockpit's one
/// settings button: the vario, airspace, the task, live
/// tracking, the offline map, the glider; the lock; and the replay of the
/// sample flight while nothing is recorded. In the flight theme, a row of
/// a finger in a glove each. What goes wrong among these is said in the
/// cockpit's top line, not here.
struct CockpitSettingsSheet: View {
    /// Makes the screen deaf to touches until the lock is held.
    let lock: () -> Void

    @Environment(FlySession.self) private var fly
    @Environment(AirspaceModel.self) private var airspace
    @Environment(TaskModel.self) private var tasks
    @Environment(AccountModel.self) private var account
    @Environment(OfflineMapsModel.self) private var maps
    @Environment(\.dismiss) private var dismiss
    @AppStorage(Preferences.liveAudience) private var liveAudience: String?
    @AppStorage(Preferences.cockpitHidesThermalMap) private var hidesThermalMap = false

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    NavigationLink {
                        VarioDeviceList(vario: fly.vario)
                    } label: {
                        LabeledContent {
                            Text(varioSummary)
                        } label: {
                            Label("Vario", image: "icon-vario")
                        }
                    }
                    .accessibilityIdentifier("vario-before-takeoff")
                } footer: {
                    // When the phone sounds is the core's: in flight only,
                    // and not while a connected vario beeps itself.
                    Text("The phone beeps in flight, and not while a connected vario beeps itself.")
                }
                if MapStyle.appDomain != nil {
                    Section {
                        // Where thermals are found, from thermal.kk7.ch,
                        // over the cockpit's map.
                        Toggle(isOn: Binding(get: { !hidesThermalMap }, set: { hidesThermalMap = !$0 })) {
                            Label(mapLayersWords().thermals, image: "icon-thermal")
                        }
                        .accessibilityIdentifier("cockpit-thermal-map")
                    }
                }
                Section {
                    // What the cockpit will warn from.
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
                    NavigationLink {
                        CockpitSettingsScreen()
                    } label: {
                        Label(pagesWords().gliderAndCockpit, image: "icon-wing")
                    }
                    .accessibilityIdentifier("glider-before-takeoff")
                }
                Section {
                    // Nothing on the screen answers a touch until it is
                    // unlocked by holding a finger on the lock.
                    Button {
                        dismiss()
                        lock()
                    } label: {
                        Label("Lock the screen", systemImage: "lock.fill")
                    }
                    .accessibilityIdentifier("cockpit-lock")
                }
                if offersReplay {
                    Section {
                        if LaunchOptions.replayFrom != nil {
                            // A UI test's moment of the flight: no menu,
                            // which XCTest cannot tap over the cockpit.
                            Button {
                                replay(speed: 20)
                            } label: {
                                Label("Replay the sample flight", systemImage: "play.fill")
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .contentShape(.rect)
                            }
                            .accessibilityIdentifier("replay-sample")
                        } else {
                            Menu {
                                Button("In real time") {
                                    replay(speed: 1)
                                }
                                Button("20 times faster") {
                                    replay(speed: 20)
                                }
                            } label: {
                                // The whole row opens the menu, not its words
                                // alone: on a wide iPhone a tap in the row's
                                // middle fell beside the words and opened nothing.
                                Label("Replay the sample flight", systemImage: "play.fill")
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .contentShape(.rect)
                            }
                            .accessibilityIdentifier("replay-sample")
                        }
                    } footer: {
                        Text("Plays a recorded flight through the cockpit, to try it on the ground. Nothing is saved.")
                    }
                }
            }
            .contentMargins(.top, VentiSpace.s2, for: .scrollContent)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(commonWords().done) {
                        dismiss()
                    }
                }
            }
        }
    }

    /// The replay is offered while the cockpit waits on the ground and
    /// records nothing; a replay running offers it again. Read from what
    /// changes seldom: the sheet must not be drawn anew with every refresh
    /// of the cockpit, or its menu never comes to rest.
    private var offersReplay: Bool {
        switch fly.mode {
        case .live: !fly.isRecording
        case .replay: true
        case nil: false
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
        let choice = LiveChoice.chosen(liveAudience)
        return choice == .nobody || account.isSignedIn ? choice.short : "Not signed in"
    }

    /// The sample flight through the cockpit, in place of the session
    /// that waited on the ground.
    private func replay(speed: Double) {
        dismiss()
        guard let url = Bundle.main.url(forResource: "sample", withExtension: "igc"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else {
            fly.problem = "The sample flight is missing from the app."
            return
        }
        if case .replay = fly.mode {
            _ = fly.finish()
        }
        fly.startReplay(
            igcText: text, speed: speed, fromS: LaunchOptions.replayFrom ?? 0, holdingAfterS: LaunchOptions.replayFor
        )
    }
}

/// The list of Bluetooth devices to pick a vario from, a page of the
/// cockpit's settings: where the pilot chooses between the phone and an
/// external vario (a vario remembered is the core's `VarioChoice.external`).
struct VarioDeviceList: View {
    let vario: VarioLink

    var body: some View {
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
                            .font(.ventiType(.label))
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
                    .foregroundStyle(.venti(.text))
                }
            } header: {
                HStack {
                    Text("Nearby devices")
                    if vario.state == .scanning {
                        ProgressView()
                    }
                }
            } footer: {
                Text("Without an external vario the phone's own pressure sensor is used. Switch the vario on and choose it here, not in the iPhone's Bluetooth settings. Venti reads LK8EX1, OpenVario, XC Tracer, LXWP0 and BlueFly data.")
            }
        }
        .navigationTitle("Vario")
        .navigationBarTitleDisplayMode(.inline)
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
