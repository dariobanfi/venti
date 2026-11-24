import SwiftUI
import VentiCore

/// The Fly tab: the cockpit. Choosing the tab opens it, waiting on the
/// ground and recording from the take-off; what was checked before
/// take-off (the vario, airspace, the task, live tracking, the offline
/// map, the glider) is behind the cockpit's one settings button. Ended,
/// the cockpit leaves this tab at rest, with the way back into it.
struct FlyScreen: View {
    @Environment(LogbookModel.self) private var logbook
    @Environment(AppNavigation.self) private var navigation
    @Environment(FlySession.self) private var fly
    @Environment(WaypointsModel.self) private var waypoints
    @Environment(AirspaceModel.self) private var airspace
    @Environment(TaskModel.self) private var tasks
    @Environment(OfflineMapsModel.self) private var maps

    /// Where the pilot is, read without asking once the pilot has allowed
    /// it: the airspace the cockpit warns from is asked for around it.
    @State private var position = PilotPosition()
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        NavigationStack {
            VentiList {
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
        // The tab is the cockpit: choosing it opens the cockpit, which
        // waits for the take-off. Once on screen, as a change of the tab:
        // a cockpit asked for before this tab is there has nothing to open
        // over, and stays shut. A replay asked for elsewhere, or a take-off
        // seen on another tab, has the cockpit open already.
        .onChange(of: navigation.tab == .fly, initial: true) { _, isInFront in
            if isInFront, !fly.isCockpitOpen {
                fly.startFlight()
            }
        }
        // The airspace around the pilot, asked for by this tab itself and
        // kept on the iPhone for a flight without a network: when the tab
        // is shown and when the pilot has moved on, as the core says it is
        // due. The cockpit must not depend on the map tab having drawn
        // airspace, which is the pilot's choice of what that map shows.
        .onChange(of: scenePhase, initial: true) { _, phase in
            if phase == .active {
                position.readIfAllowed()
            }
        }
        .task(id: AirspaceModel.askedKey(position.place)) {
            position.readIfAllowed()
            if let place = position.place {
                await airspace.keepCovered(around: place)
            }
        }
        .fullScreenCover(
            isPresented: Binding(
                get: { fly.isCockpitOpen },
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
                get: { fly.problem != nil && !fly.isCockpitOpen },
                set: { isPresented in
                    if !isPresented {
                        fly.problem = nil
                    }
                }
            ),
            presenting: fly.problem
        ) { _ in
            Button(commonWords().ok, role: .cancel) {}
        } message: { problem in
            Text(problem)
        }
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
