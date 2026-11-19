import SwiftUI
import VentiCore

/// The line between the map and the instruments that says where the
/// cockpit is navigating to: which way to turn, how far, and how high the
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
                            .font(.headline)
                            .lineLimit(1)
                        Text("\(Format.kilometres(metres: way.distanceM)) km")
                            .font(.system(.subheadline, design: .rounded, weight: .medium))
                            .monospacedDigit()
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 8)
                    arrival(way)
                } else {
                    Image(systemName: "mappin.and.ellipse")
                        .font(.title3)
                        .frame(width: 30)
                    Text("Choose a place to fly to")
                        .font(.headline)
                    Spacer(minLength: 8)
                }
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
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
            .font(.title2)
            .foregroundStyle(Palette.accent)
            .rotationEffect(.degrees(way.bearingDeg - (courseDeg ?? 0)))
            .frame(width: 30)
    }

    /// The height the glider is expected to arrive at, over the target.
    @ViewBuilder
    private func arrival(_ way: WayToTarget) -> some View {
        VStack(alignment: .trailing, spacing: 1) {
            if let height = way.arrivalHeightM {
                Text("\(Text(Format.signedMetres(height)).font(.system(.title3, design: .rounded, weight: .semibold)))\(Text(" m").font(.system(.footnote, design: .rounded, weight: .medium)).foregroundStyle(.secondary))")
                    .monospacedDigit()
                    // Falling short is the one thing here worth a warning colour.
                    .foregroundStyle(height < 0 ? Color.red : Color.primary)
                Text(way.windKnown ? "Arrival" : "Arrival in still air")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if way.expectedGlide == nil {
                Text("Wind too strong")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.red)
                Text("to get there")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .lineLimit(1)
    }

    private var spoken: String {
        guard let way else { return "Nothing chosen" }
        var parts = ["\(way.name), \(Format.kilometres(metres: way.distanceM)) kilometres"]
        if let height = way.arrivalHeightM {
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
    /// The place the cockpit is navigating to now; `nil` for the take-off.
    let chosen: String?
    /// Called with the chosen place; `nil` for the take-off.
    let choose: (WaypointView?) -> Void

    @Environment(WaypointsModel.self) private var waypoints
    @Environment(\.dismiss) private var dismiss

    @State private var search = ""
    @State private var isChoosingFiles = false
    @State private var notice: ImportNotice?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Button {
                        choose(nil)
                        dismiss()
                    } label: {
                        HStack(spacing: 12) {
                            Image(systemName: "house.circle")
                                .font(.title3)
                                .foregroundStyle(.secondary)
                                .frame(width: 28)
                            Text("Take-off")
                            Spacer(minLength: 8)
                            if chosen == nil {
                                Image(systemName: "checkmark")
                                    .font(.body.weight(.semibold))
                                    .foregroundStyle(Palette.accent)
                            }
                        }
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                } footer: {
                    Text("Without a place chosen, the cockpit shows the way back to where you took off.")
                }

                Section("Waypoints") {
                    let places = shown
                    ForEach(places, id: \.id) { waypoint in
                        Button {
                            choose(waypoint)
                            dismiss()
                        } label: {
                            WaypointRow(waypoint: waypoint, isChosen: waypoint.name == chosen)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("target-row")
                    }
                    if waypoints.all.isEmpty {
                        Text("You have no waypoints yet. Venti reads SeeYou files (.cup) and GEO files (.wpt).")
                            .foregroundStyle(.secondary)
                    } else if places.isEmpty {
                        Text("No waypoint matches “\(search)”.")
                            .foregroundStyle(.secondary)
                    }
                    Button("Add waypoints from a file", systemImage: "square.and.arrow.down") {
                        isChoosingFiles = true
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
