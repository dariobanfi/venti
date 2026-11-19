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
        List {
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
                ContentUnavailableView {
                    Label("No waypoints yet", systemImage: "mappin.and.ellipse")
                } description: {
                    Text("Bring in a waypoint file to fly to its places in the cockpit. Venti reads SeeYou files (.cup) and GEO files (.wpt).")
                } actions: {
                    Button("Choose a file") {
                        isChoosingFiles = true
                    }
                    .buttonStyle(.borderedProminent)
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
                Menu("More", systemImage: "ellipsis") {
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
struct WaypointRow: View {
    let waypoint: WaypointView
    var isChosen = false

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: waypoint.symbolName)
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(waypoint.name)
                    .lineLimit(1)
                if !waypoint.detail.isEmpty {
                    Text(waypoint.detail)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            if isChosen {
                Image(systemName: "checkmark")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.accent)
            }
        }
        .contentShape(.rect)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(waypoint.name)
        .accessibilityValue(isChosen ? "Chosen. \(waypoint.detail)" : waypoint.detail)
    }
}
