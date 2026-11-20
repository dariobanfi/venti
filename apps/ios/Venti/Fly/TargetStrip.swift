import SwiftUI
import VentiCore

/// The line under the cockpit's map that says where the cockpit is
/// navigating to: which way to turn, how far, and how high the
/// glider is expected to arrive. A tap chooses another place.
struct TargetStrip: View {
    /// The way to the target, if there is one to go to yet.
    let way: WayToTarget?
    /// The direction the glider is travelling, if it is moving.
    let courseDeg: Double?
    let choose: () -> Void

    var body: some View {
        Button(action: choose) {
            HStack(spacing: 12) {
                if let way {
                    arrow(way)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(way.name)
                            .font(.venti(.headline))
                            .lineLimit(1)
                        Text("\(Format.kilometres(metres: way.distanceM)) km")
                            .font(.venti(.subheadline, .medium))
                            .monospacedDigit()
                            .foregroundStyle(.venti(.textSecondary))
                    }
                    Spacer(minLength: 8)
                    arrival(way)
                } else {
                    Image(systemName: "mappin.and.ellipse")
                        .font(.venti(.title3))
                        .frame(width: 30)
                    Text("Choose a place to fly to")
                        .font(.venti(.headline))
                    Spacer(minLength: 8)
                }
                Image(systemName: "chevron.right")
                    .font(.venti(.subheadline, .semibold))
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 9)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Flying to")
        .accessibilityValue(spoken)
        .accessibilityHint("Chooses another place")
        .accessibilityIdentifier("target-strip")
    }

    /// An arrow that points to the target as seen from the glider: up
    /// means straight ahead. Standing still, up means north.
    private func arrow(_ way: WayToTarget) -> some View {
        Image(systemName: "location.north.fill")
            .font(.venti(.title2))
            .foregroundStyle(Palette.accent)
            .rotationEffect(.degrees(way.bearingDeg - (courseDeg ?? 0)))
            .frame(width: 30)
    }

    /// The height the glider is expected to arrive at, over the target.
    @ViewBuilder
    private func arrival(_ way: WayToTarget) -> some View {
        VStack(alignment: .trailing, spacing: 1) {
            if let meets = way.groundInTheWayM {
                // The straight glide would arrive with height to spare,
                // and meets a ridge on the way: that is what counts.
                Text("Ground in the way")
                    .font(.venti(.subheadline, .semibold))
                    .foregroundStyle(.venti(.danger))
                Text("in \(Format.kilometres(metres: meets)) km")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
                    .monospacedDigit()
            } else if let height = way.arrivalHeightM {
                Text("\(Text(Format.signedMetres(height)).font(.venti(.title3, .semibold)))\(Text(" m").font(.venti(.subheadline, .medium)))")
                    .monospacedDigit()
                    // Falling short is the one thing here worth a warning colour.
                    .foregroundStyle(.venti(height < 0 ? .danger : .text))
                Text(way.windKnown ? "Arrival" : "Arrival in still air")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
            } else if way.expectedGlide == nil {
                Text("Wind too strong")
                    .font(.venti(.subheadline, .semibold))
                    .foregroundStyle(.venti(.danger))
                Text("to get there")
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.textSecondary))
            }
        }
        .lineLimit(1)
    }

    private var spoken: String {
        guard let way else { return "Nothing chosen" }
        var parts = ["\(way.name), \(Format.kilometres(metres: way.distanceM)) kilometres"]
        if let meets = way.groundInTheWayM {
            parts.append("the ground is in the way in \(Format.kilometres(metres: meets)) kilometres")
        } else if let height = way.arrivalHeightM {
            parts.append(
                height < 0
                    ? "expected to fall short by \(Format.metres(-height)) metres"
                    : "expected to arrive \(Format.metres(height)) metres above it"
            )
        } else if way.expectedGlide == nil {
            parts.append("the wind is too strong to get there")
        }
        return parts.joined(separator: ", ")
    }
}

/// Where to fly to: the take-off, or one of the pilot's waypoints, nearest
/// first.
struct TargetPicker: View {
    /// Where the glider is, to list the nearest places first.
    let position: Coordinate?
    /// The place the pilot chose to fly to; `nil` when none is chosen and
    /// the cockpit leads along the task or back to the take-off.
    let chosen: String?
    /// Whether a task is being flown. Without a place chosen the cockpit
    /// then leads to the task's next turnpoint, not to the take-off.
    var fliesTask = false
    /// Called with the chosen place; `nil` for none.
    let choose: (WaypointView?) -> Void

    @Environment(WaypointsModel.self) private var waypoints
    @Environment(FlySession.self) private var fly
    @Environment(\.dismiss) private var dismiss

    @State private var search = ""
    @State private var isChoosingFiles = false
    @State private var notice: ImportNotice?

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Button {
                        choose(nil)
                        dismiss()
                    } label: {
                        HStack(spacing: 12) {
                            Image(systemName: fliesTask ? "flag.checkered" : "house.circle")
                                .font(.venti(.title3))
                                .foregroundStyle(.venti(.textSecondary))
                                .frame(width: 28)
                            Text(fliesTask ? "Along the task" : "Take-off")
                            Spacer(minLength: 8)
                            if chosen == nil {
                                Image(systemName: "checkmark")
                                    .font(.venti(.body, .semibold))
                                    .foregroundStyle(Palette.accent)
                            }
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                } footer: {
                    Text(
                        fliesTask
                            ? "Without a place chosen, the cockpit leads to the task's next turnpoint."
                            : "Without a place chosen, the cockpit shows the way back to where you took off."
                    )
                }

                Section {
                    let places = shown
                    // How each lies within the glider's reach right now.
                    let reach = fly.reach(of: places)
                    ForEach(Array(places.enumerated()), id: \.element.id) { index, waypoint in
                        Button {
                            choose(waypoint)
                            dismiss()
                        } label: {
                            WaypointRow(
                                waypoint: waypoint, isChosen: waypoint.name == chosen,
                                reach: reach.indices.contains(index) ? reach[index] : nil
                            )
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("target-row")
                    }
                    if waypoints.all.isEmpty {
                        Text("You have no waypoints yet. Venti reads SeeYou files (.cup) and GEO files (.wpt).")
                            .foregroundStyle(.venti(.textSecondary))
                    } else if places.isEmpty {
                        Text("No waypoint matches “\(search)”.")
                            .foregroundStyle(.venti(.textSecondary))
                    }
                    Button("Add waypoints from a file", systemImage: "square.and.arrow.down") {
                        isChoosingFiles = true
                    }
                } header: {
                    Text("Waypoints")
                } footer: {
                    if !waypoints.all.isEmpty {
                        Text("The height beside a place is what the glider is expected to arrive with, by its trim speed and glide ratio from the settings and the wind last measured, in a straight line. Where the iPhone has the ground, a place with a ridge in the way says “Ground in the way” and counts as out of reach; where it has none, the ground in between is not looked at.")
                    }
                }
            }
            .searchable(text: $search, prompt: "Waypoint")
            .navigationTitle("Fly to")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
            }
            .fileImporter(
                isPresented: $isChoosingFiles,
                allowedContentTypes: [.data],
                allowsMultipleSelection: true
            ) { result in
                guard case .success(let urls) = result else { return }
                Task {
                    notice = await waypoints.importFiles(at: urls)
                }
            }
            .importNotice($notice)
        }
    }

    private var shown: [WaypointView] {
        let wanted = search.trimmingCharacters(in: .whitespaces)
        // Asked for again whenever the list is drawn, so new waypoints show.
        _ = waypoints.all
        let near = waypoints.near(position)
        guard !wanted.isEmpty else { return near }
        return near.filter {
            $0.name.localizedCaseInsensitiveContains(wanted) || $0.code.localizedCaseInsensitiveContains(wanted)
        }
    }
}
