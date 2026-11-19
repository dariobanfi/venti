import SwiftUI
import VentiCore

/// The pilot's airspace files: bring one in, see what it gave, remove it.
struct AirspaceScreen: View {
    @Environment(AirspaceModel.self) private var airspace

    @State private var isChoosingFiles = false
    @State private var notice: ImportNotice?

    var body: some View {
        List {
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
            } footer: {
                Text("""
                    Venti reads airspace files in OpenAir format, the kind other flight instruments use. It draws their airspace on the maps and warns in the cockpit when you come near, are about to enter or are inside.

                    It knows the airspace in these files and no other. Where you have no file, or an old one, it knows nothing and stays silent. It does not know which areas are active today. Every warning is advice: you remain responsible for where you fly.
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

    private func detail(of file: AirspaceFile) -> String {
        let count = file.airspaces == 1 ? "1 airspace" : "\(file.airspaces.formatted()) airspaces"
        guard let imported = file.importedUnixS else { return count }
        return "\(count) · brought in \(Format.day(unixSeconds: imported))"
    }
}
