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

    @State private var isChoosingVario = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Button {
                        isChoosingVario = true
                    } label: {
                        LabeledContent {
                            Text(varioSummary)
                        } label: {
                            Label("Vario", systemImage: "dot.radiowaves.left.and.right")
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
                            Label("Airspace", systemImage: "square.dashed")
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
                } footer: {
                    Text(
                        airspace.isEmpty
                            ? "Without an external vario the phone's own pressure sensor is used. It is slower. Without airspace the cockpit warns of none."
                            : "Without an external vario the phone's own pressure sensor is used. It is slower."
                    )
                }

                Section {
                    Button("Start flight") {
                        fly.startFlight()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                } footer: {
                    Text("Recording starts at once and carries on with the screen off. When you finish, the flight is saved to the logbook.")
                }

                Section {
                    Menu("Replay the sample flight") {
                        Button("In real time") {
                            startReplay(speed: 1)
                        }
                        Button("20 times faster") {
                            startReplay(speed: 20)
                        }
                    }
                } footer: {
                    Text("Plays a recorded flight through the cockpit, to try it on the ground. Nothing is saved.")
                }
            }
            .navigationTitle("Fly")
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
        }
        .alert(
            "Flying",
            isPresented: Binding(
                get: { fly.problem != nil },
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
            List {
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
                        LabeledContent("Received", value: "\(vario.report.bytes.formatted()) bytes")
                        ForEach(Array(vario.report.lines.enumerated()), id: \.offset) { line in
                            Text(line.element)
                                .font(.caption.monospaced())
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
                    Text("Switch the vario on and choose it here, not in the iPhone's Bluetooth settings. Venti reads LK8EX1, OpenVario, XC Tracer, LXWP0 and BlueFly data.")
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
