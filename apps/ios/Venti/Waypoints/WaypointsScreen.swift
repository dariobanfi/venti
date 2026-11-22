import SwiftUI
import VentiCore

/// The pilot's waypoints: bring a file in, look through them, remove them,
/// take them out again as a file.
struct WaypointsScreen: View {
    @Environment(WaypointsModel.self) private var waypoints

    @State private var search = ""
    @State private var isChoosingFiles = false
    @State private var isConfirmingDeleteAll = false
    @State private var notice: ImportNotice?
    @State private var exported: URL?

    var body: some View {
        VentiList {
            ForEach(shown, id: \.id) { waypoint in
                WaypointRow(waypoint: waypoint)
                    .swipeActions {
                        Button("Delete", role: .destructive) {
                            waypoints.delete(id: waypoint.id)
                        }
                    }
            }
        }
        .overlay {
            if waypoints.all.isEmpty {
                VentiEmpty(
                    title: "No waypoints yet",
                    text: "Bring in a waypoint file to fly to its places in the cockpit. Venti reads SeeYou files (.cup) and GEO files (.wpt)."
                ) {
                    Button {
                        isChoosingFiles = true
                    } label: {
                        Text("Choose a file")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.primary))
                }
            } else if shown.isEmpty {
                ContentUnavailableView.search(text: search)
            }
        }
        .searchable(text: $search, prompt: "Waypoint")
        .navigationTitle("Waypoints")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                VentiBarMenu(title: "More") {
                    Button("Add from a file", systemImage: "square.and.arrow.down") {
                        isChoosingFiles = true
                    }
                    if let exported, !waypoints.all.isEmpty {
                        ShareLink("Share as a file", item: exported)
                    }
                    if !waypoints.all.isEmpty {
                        Button("Delete all", systemImage: "trash", role: .destructive) {
                            isConfirmingDeleteAll = true
                        }
                    }
                }
            }
        }
        // The file to share is written whenever the waypoints change.
        .task(id: waypoints.all) {
            exported = nil
            if !waypoints.all.isEmpty {
                exported = try? waypoints.exportFile()
            }
        }
        // Waypoint files have no type the system knows, so any file can be
        // picked; the core says which ones hold waypoints.
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
        .confirmationDialog(
            "Delete all waypoints?",
            isPresented: $isConfirmingDeleteAll,
            titleVisibility: .visible
        ) {
            Button("Delete all", role: .destructive) {
                waypoints.deleteAll()
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

/// One waypoint in a line.
extension WaypointRow {
    /// What a row says to someone who cannot see it.
    static func spoken(_ waypoint: WaypointView, isChosen: Bool, reach: ReachView?) -> String {
        var parts: [String] = []
        if isChosen {
            parts.append("Chosen.")
        }
        parts.append(waypoint.detail)
        if reach?.groundInTheWayM != nil {
            parts.append("Out of reach: the ground is in the way.")
        } else if let reach, let arrival = reach.arrivalHeightM {
            switch reach.reach {
            case .within, .just:
                parts.append("Arriving \(Format.signedMetres((arrival / 10).rounded() * 10)) metres above it.")
            case .beyond:
                parts.append("Out of reach.")
            case .unknown:
                break
            }
        } else if reach?.reach == .beyond {
            parts.append("Out of reach.")
        }
        return parts.joined(separator: " ")
    }
}

struct WaypointRow: View {
    let waypoint: WaypointView
    var isChosen = false
    /// How it lies within the glider's reach, in the cockpit's list.
    var reach: ReachView?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: waypoint.symbolName)
                .font(.venti(.title3))
                .foregroundStyle(.secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(waypoint.name)
                    .lineLimit(1)
                if !waypoint.detail.isEmpty {
                    Text(waypoint.detail)
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            // The height the glider would arrive with: plain with height
            // to spare, amber with little, red when it comes down before.
            if reach?.groundInTheWayM != nil {
                // A straight glide would get there, and a ridge is in
                // its way: the height it would arrive with says nothing.
                Label("Ground in the way", systemImage: "mountain.2.fill")
                    .labelStyle(.iconOnly)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.venti(.danger))
            } else if let reach, let arrival = reach.arrivalHeightM, reach.reach != .unknown {
                Text("\(Format.signedMetres((arrival / 10).rounded() * 10)) m")
                    .font(.venti(.subheadline, .medium))
                    .monospacedDigit()
                    .foregroundStyle(.venti(LandingBadge.role(reach.reach)))
            }
            if isChosen {
                Image(systemName: "checkmark")
                    .font(.venti(.body, .semibold))
                    .foregroundStyle(Palette.accent)
            }
        }
        .contentShape(.rect)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(waypoint.name)
        .accessibilityValue(Self.spoken(waypoint, isChosen: isChosen, reach: reach))
    }
}
