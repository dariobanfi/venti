import SwiftUI
import VentiCore

/// The pilot's airspace: what the app keeps from the pilot's server, and
/// the files the pilot brings in. Where each comes from, how old it is,
/// and what it left out.
struct AirspaceScreen: View {
    @Environment(AirspaceModel.self) private var airspace
    @Environment(WindModel.self) private var wind

    @State private var isChoosingFiles = false
    @State private var notice: ImportNotice?

    var body: some View {
        List {
            if let problem = airspace.problem {
                Section {
                    Label(problem, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                }
            }
            fromServer
            Section {
                ForEach(airspace.files, id: \.storedName) { file in
                    VStack(alignment: .leading, spacing: 3) {
                        Text(file.name)
                        Text(detail(of: file))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        if file.leftOut > 0 {
                            Label(
                                file.leftOut == 1
                                    ? "1 airspace in this file could not be read. You will not be warned about it."
                                    : "\(file.leftOut) airspaces in this file could not be read. You will not be warned about them.",
                                systemImage: "exclamationmark.triangle.fill"
                            )
                            .font(.footnote)
                            .foregroundStyle(.red)
                        }
                    }
                    .padding(.vertical, 2)
                    .swipeActions {
                        Button("Delete", role: .destructive) {
                            airspace.delete(file)
                        }
                    }
                }
                Button("Add an airspace file", systemImage: "square.and.arrow.down") {
                    isChoosingFiles = true
                }
            } header: {
                Text("Your files")
            } footer: {
                Text("""
                    Venti reads airspace files in OpenAir format, the kind other flight instruments use. It draws their airspace on the maps and warns in the cockpit when you come near, are about to enter or are inside.

                    It knows the airspace listed here and no other. Where none of it reaches, the cockpit says that nothing is checked there; an old file it cannot tell from a new one. It does not know which areas are active today. For Germany and Austria it knows the rules a paraglider flies under; anywhere else it goes by an airspace's class and type alone. Every warning is advice: you remain responsible for where you fly.
                    """)
            }
        }
        .navigationTitle("Airspace")
        .navigationBarTitleDisplayMode(.inline)
        // Airspace files have no type the system knows, so any file can be
        // picked; the core says which ones hold airspace.
        .fileImporter(
            isPresented: $isChoosingFiles,
            allowedContentTypes: [.data],
            allowsMultipleSelection: true
        ) { result in
            guard case .success(let urls) = result else { return }
            Task {
                notice = await airspace.importFiles(at: urls)
            }
        }
        .importNotice($notice)
    }

    /// What is kept from the server, with the credit and the notice its
    /// provider asks for, and the switch that stops it.
    private var fromServer: some View {
        Section {
            ForEach(airspace.kept, id: \.source) { layer in
                VStack(alignment: .leading, spacing: 4) {
                    Text(layer.provider)
                    Text(detail(of: layer))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    if !layer.countries.isEmpty {
                        // The kept area reaches across borders, and the
                        // cockpit cannot draw one: say whose airspace this is.
                        Text("The airspace of \(Self.names(of: layer.countries)). Of other countries none is kept, also where the area fetched reaches into them.")
                            .font(.footnote)
                    }
                    if !layer.saysWhatItCovers {
                        Label(
                            "The server does not say for which area this is complete, so the cockpit cannot take silence for \"no airspace near\" and says that it is not checking.",
                            systemImage: "exclamationmark.triangle.fill"
                        )
                        .font(.footnote)
                        .foregroundStyle(.red)
                    }
                    if let credit = layer.credit {
                        if let notice = credit.notice {
                            Label(notice, systemImage: "exclamationmark.triangle")
                                .font(.footnote)
                        }
                        Text("\(credit.text) Licence: \(credit.licence).")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 2)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("airspace-from-server")
            }
            if let problem = airspace.serverProblem {
                Label(problem, systemImage: "wifi.exclamationmark")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Toggle(
                "Airspace from your server",
                isOn: Binding(
                    get: { airspace.usesServer },
                    set: { isOn in
                        if isOn {
                            airspace.usesServer = true
                        } else {
                            airspace.stopUsingServer()
                        }
                    }
                )
            )
        } header: {
            Text("From your server")
        } footer: {
            Text(serverNote)
        }
    }

    private var serverNote: String {
        if !airspace.usesServer {
            return "Switched off: only your own files are used."
        }
        if wind.server == nil {
            return "With a Venti server set in the settings, the app fetches the airspace around you by itself and keeps it on this iPhone, so the cockpit warns without a network."
        }
        if airspace.kept.isEmpty {
            return "Fetched the next time the map is open or a flight starts with a network, for about 200 km around you, and kept on this iPhone."
        }
        return "Kept on this iPhone for about 200 km around where it was fetched, so the cockpit warns without a network. Fetched again once a day, and when you are somewhere else."
    }

    /// Countries by their names in the pilot's language: "Germany and
    /// Austria". A code the iPhone does not know stays as it is.
    static func names(of countries: [String]) -> String {
        countries
            .map { Locale.current.localizedString(forRegionCode: $0) ?? $0 }
            .formatted(.list(type: .and))
    }

    private func detail(of layer: KeptAirspace) -> String {
        var parts = [AirspaceModel.count(layer.airspaces)]
        if let written = layer.writtenUnixS {
            parts.append("as of \(Format.day(unixSeconds: written))")
        }
        parts.append("fetched \(Format.day(unixSeconds: layer.keptUnixS))")
        return parts.joined(separator: " · ")
    }

    private func detail(of file: AirspaceFile) -> String {
        let count = file.airspaces == 1 ? "1 airspace" : "\(Format.count(file.airspaces)) airspaces"
        guard let imported = file.importedUnixS else { return count }
        return "\(count) · brought in \(Format.day(unixSeconds: imported))"
    }
}
