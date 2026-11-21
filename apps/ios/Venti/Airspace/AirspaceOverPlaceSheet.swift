import SwiftUI
import VentiCore

/// A place of the map where airspace is drawn, tapped: what lies over it.
struct ChosenAirspacePlace: Identifiable {
    let over: AirspaceOverPlace

    var id: String {
        "\(over.position.lat),\(over.position.lon)"
    }
}

/// What lies over a place, as a sheet named by the place: the core's line
/// at the head, each airspace from the lowest floor up with how Venti
/// treats it and what its source publishes about its use, the caveat that
/// the data is no clearance, and at the foot the filter by height for the
/// app's maps. The words are the core's; the order is the core's.
struct AirspaceOverPlaceSheet: View {
    let over: AirspaceOverPlace

    @Environment(\.dismiss) private var dismiss
    @AppStorage(Preferences.mapAirspaceCeilingM) private var ceilingM = 0.0

    private let filter = airspaceHeightFilter()

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    Text(over.words)
                        .font(.venti(.headline))
                        .accessibilityIdentifier("airspace-over-words")
                    if let cover = over.coverNote {
                        Text(cover)
                            .foregroundStyle(.secondary)
                    }
                }
                ForEach(over.airspaces, id: \.id) { airspace in
                    Section {
                        AirspaceOverheadRow(airspace: airspace)
                    }
                }
                Section {
                    if let hours = over.hoursNote {
                        Text(hours)
                    }
                    Text(over.caveat)
                        .accessibilityIdentifier("airspace-over-caveat")
                }
                .font(.venti(.footnote))
                .foregroundStyle(.secondary)
                // The maps' filter by height, in the core's words.
                Section {
                    VentiSegmented(
                        options: filter.steps.map { (value: $0.ceilingM ?? 0, name: $0.words) },
                        selection: $ceilingM
                    )
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel(filter.title)
                    .accessibilityIdentifier("airspace-height-filter")
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                } header: {
                    Text(filter.title)
                } footer: {
                    Text(filter.note)
                }
            }
            .navigationTitle(Format.place(lat: over.position.lat, lon: over.position.lon))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
        .accessibilityIdentifier("airspace-over")
    }
}

/// One airspace over the place: its name and what it is, floor to ceiling
/// as published, how Venti treats it and why, what its source publishes
/// about its use and its hours, its remarks, and where it is from.
private struct AirspaceOverheadRow: View {
    let airspace: AirspaceOverhead

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(airspace.name)
                        .font(.venti(.headline))
                    Text(airspace.kind)
                        .font(.venti(.subheadline))
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: VentiSpace.s2)
                treatment
            }
            Text("\(airspace.lower) to \(airspace.upper)")
                .monospacedDigit()
            Text(airspace.why)
                .font(.venti(.subheadline))
            if let use = airspace.publishedUse {
                Text(use)
                    .font(.venti(.subheadline))
            }
            ForEach(airspace.hours, id: \.self) { line in
                Text(line)
                    .font(.venti(.subheadline))
                    .monospacedDigit()
            }
            if let remarks = airspace.hoursRemarks {
                Text(remarks)
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
            if let remarks = airspace.remarks {
                Text(remarks)
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
            Text(AirspaceWarningsSheet.origin(of: airspace))
                .font(.venti(.footnote))
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, VentiSpace.s1)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("airspace-over-row")
    }

    /// How Venti treats it: the pack's status for one to avoid or to take
    /// care in, sign, word and colour; plain words for one that is free,
    /// which is a fact about the rules and no promise.
    @ViewBuilder private var treatment: some View {
        switch airspace.policy {
        case .avoid:
            VentiStatus(kind: .danger, word: airspace.policyWords)
        case .caution:
            VentiStatus(kind: .caution, word: airspace.policyWords)
        case .free:
            Text(airspace.policyWords)
                .font(.ventiType(.label))
                .foregroundStyle(.venti(.textSecondary))
        }
    }
}
