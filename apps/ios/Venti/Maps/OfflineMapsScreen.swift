import SwiftUI
import VentiCore

/// Maps for flying without a network: the regions on this iPhone and the
/// ones the pilot's Venti server hands out, with what a download does.
///
/// Reached from the Fly tab's list of what to check before take-off, and
/// from the settings.
struct OfflineMapsScreen: View {
    @Environment(OfflineMapsModel.self) private var maps
    @Environment(WindModel.self) private var wind

    /// The region the pilot is asked about before it is downloaded.
    @State private var asking: MapRegionView?
    /// The region the pilot is asked about before it is deleted.
    @State private var deleting: MapRegionView?

    var body: some View {
        VentiList {
            ForEach(maps.rows, id: \.region.id) { row in
                Section {
                    MapRegionLines(row: row, progress: maps.progress[row.region.id])
                    actions(for: row)
                }
            }

            if maps.rows.isEmpty {
                Section {
                    Text(emptyWords)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("maps-none")
                }
            } else if let problem = maps.problem {
                // The iPhone's own regions are listed; the server's are not.
                Section {
                    Text(problem)
                        .foregroundStyle(.secondary)
                }
            }

            Section {
                EmptyView()
            } footer: {
                Text("A downloaded region is drawn from this iPhone: in the cockpit wherever the glider is inside it, with a network or without, and on the other maps when there is no network. The height of the ground comes from it too. Outside the region the map comes from the network as before. A download runs while Venti is open; if it stops, it goes on from where it was.")
            }

            if !maps.kept.isEmpty {
                Section("Whose data the downloaded maps hold") {
                    ForEach(credits, id: \.text) { credit in
                        if let link = credit.url, let url = URL(string: link) {
                            Link(credit.text, destination: url)
                        } else {
                            Text(credit.text)
                        }
                    }
                }
            }
        }
        .navigationTitle("Offline maps")
        .navigationBarTitleDisplayMode(.inline)
        .task(id: wind.address) {
            await maps.refresh()
        }
        .refreshable {
            await maps.refresh()
        }
        .confirmationDialog(
            asking.map { "Download \($0.name)?" } ?? "",
            isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
            titleVisibility: .visible,
            presenting: asking
        ) { region in
            Button("Download \(region.size)") {
                maps.download(region)
            }
        } message: { region in
            Text("\(region.size) from Venti. On mobile data this counts against your plan.")
        }
        .confirmationDialog(
            deleting.map { "Delete \($0.name)?" } ?? "",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible,
            presenting: deleting
        ) { region in
            Button("Delete the map", role: .destructive) {
                maps.delete(region)
            }
        } message: { _ in
            Text("The map goes from this iPhone. It can be downloaded again while Venti has it.")
        }
    }

    /// What can be done with a region, each a row of its own.
    @ViewBuilder private func actions(for row: MapRegionRow) -> some View {
        let id = row.region.id
        switch maps.progress[id] {
        case .downloading?, .checking?:
            Button("Stop the download", systemImage: "stop.circle") {
                maps.stop(id)
            }
        case .stopped?:
            Button("Go on with the download", systemImage: "arrow.down.circle") {
                maps.download(row.newer ?? row.region)
            }
            if row.state != .offered {
                delete(row.region)
            }
        case nil:
            switch row.state {
            case .offered:
                Button("Download", systemImage: "arrow.down.circle") {
                    asking = row.region
                }
                .accessibilityIdentifier("map-download")
            case .kept:
                delete(row.region)
            case .keptWithNewer:
                if let newer = row.newer {
                    Button("Download the newer map", systemImage: "arrow.down.circle") {
                        asking = newer
                    }
                }
                delete(row.region)
            }
        }
    }

    private func delete(_ region: MapRegionView) -> some View {
        Button("Delete from this iPhone", systemImage: "trash", role: .destructive) {
            deleting = region
        }
        .accessibilityIdentifier("map-delete")
    }

    /// Why the list is empty.
    private var emptyWords: String {
        if wind.server == nil {
            return "Maps for flying without a network come from Venti. Set its address in the settings."
        }
        if let problem = maps.problem {
            return problem
        }
        if maps.offered == nil {
            return "Asking Venti which maps it has."
        }
        return "This server has no maps to download."
    }

    /// The credits of every kept region, each once.
    private var credits: [MapCreditView] {
        var seen: [MapCreditView] = []
        for credit in maps.kept.flatMap(\.credits) where !seen.contains(credit) {
            seen.append(credit)
        }
        return seen
    }
}

/// A region's name, what there is to say about it, and how far its
/// download is.
private struct MapRegionLines: View {
    let row: MapRegionRow
    let progress: OfflineMapsModel.Progress?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(row.region.name)
                    .font(.venti(.body, .semibold))
                Spacer()
                if row.state != .offered {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(Palette.accent)
                        .accessibilityHidden(true)
                }
            }
            Text(row.words)
                .font(.venti(.subheadline))
                .foregroundStyle(.secondary)
            switch progress {
            case .downloading(let received, let total)?:
                ProgressView(value: Double(received), total: Double(max(total, 1)))
                Text(mapDownloadWords(received: UInt64(clamping: received), total: UInt64(clamping: total)))
                    .font(.venti(.footnote))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            case .checking?:
                ProgressView()
                Text("Checking the download")
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            case .stopped(let words)?:
                Text(words)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.danger))
            case nil:
                EmptyView()
            }
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("map-region")
    }
}
