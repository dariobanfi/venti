import SwiftUI
import VentiCore

/// Makes a route of the pilot's own waypoints: tapped in the order to fly
/// them, and kept as the task. A route has no cylinders, start or goal;
/// the cockpit leads from waypoint to waypoint.
struct RoutePlanner: View {
    @Environment(TaskModel.self) private var tasks
    @Environment(WaypointsModel.self) private var waypoints
    @Environment(\.dismiss) private var dismiss

    /// The waypoints of the route so far, in order. One may be in it
    /// twice: out to a place and back is a route too.
    @State private var route: [WaypointView] = []
    @State private var search = ""
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(Array(route.enumerated()), id: \.offset) { index, waypoint in
                        HStack(spacing: 12) {
                            Text(Format.count(index + 1))
                                .font(.footnote.weight(.semibold))
                                .monospacedDigit()
                                .frame(width: 28, height: 28)
                                .background(Color(.tertiarySystemFill), in: .circle)
                            Text(waypoint.name)
                                .lineLimit(1)
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("route-stop")
                    }
                    .onDelete { offsets in
                        route.remove(atOffsets: offsets)
                    }
                    .onMove { offsets, destination in
                        route.move(fromOffsets: offsets, toOffset: destination)
                    }
                    if route.isEmpty {
                        Text("Tap your waypoints below in the order you want to fly them.")
                            .foregroundStyle(.secondary)
                    }
                } header: {
                    Text("The route")
                } footer: {
                    if let problem {
                        Text(problem)
                            .foregroundStyle(.red)
                    } else if !route.isEmpty {
                        Text("Swipe a waypoint away to take it out.")
                    }
                }

                Section("Your waypoints") {
                    ForEach(shown, id: \.id) { waypoint in
                        Button {
                            route.append(waypoint)
                        } label: {
                            WaypointRow(waypoint: waypoint)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("route-waypoint")
                    }
                    if waypoints.all.isEmpty {
                        Text("You have no waypoints yet. Bring a waypoint file in under Settings, or keep a flying site from the map as a waypoint.")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .searchable(text: $search, prompt: "Waypoint")
            .navigationTitle("Plan a route")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Keep") {
                        if tasks.keepRoute(route) {
                            dismiss()
                        } else {
                            problem = tasks.problem
                        }
                    }
                    .disabled(route.count < 2)
                }
            }
        }
    }

    private var shown: [WaypointView] {
        let wanted = search.trimmingCharacters(in: .whitespaces)
        guard !wanted.isEmpty else { return waypoints.all }
        return waypoints.all.filter {
            $0.name.localizedCaseInsensitiveContains(wanted) || $0.code.localizedCaseInsensitiveContains(wanted)
        }
    }
}
