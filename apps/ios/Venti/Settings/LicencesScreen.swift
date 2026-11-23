import SwiftUI

/// "Open-source licences", a page of "Data and licences": the notices the
/// licences of the software inside the app ask to go with it. Three parts,
/// each folded until opened, so that the short ones are not below the
/// core's long list: the core's crates (the generated file, as it is), the
/// map engine with its licence and copyright lines, and the app's
/// libraries with the full text of their licence once. As Android's page.
struct LicencesScreen: View {
    @State private var coreOpen = false
    @State private var mapOpen = false
    @State private var librariesOpen = false

    var body: some View {
        VentiList {
            Section {
                LicencePart(title: LicenceWords.core, identifier: "licences-core", isOpen: $coreOpen)
                if coreOpen {
                    LicenceParagraphs(paragraphs: Self.texts.core)
                }
            }
            Section {
                LicencePart(title: LicenceWords.mapEngine, identifier: "licences-maplibre", isOpen: $mapOpen)
                if mapOpen {
                    LibraryLine(library: OpenSourceLicences.mapLibre)
                    LicenceParagraphs(paragraphs: Self.texts.mapLibre)
                }
            }
            Section {
                LicencePart(title: LicenceWords.libraries, identifier: "licences-libraries", isOpen: $librariesOpen)
                if librariesOpen {
                    ForEach(OpenSourceLicences.libraries, id: \.self) { library in
                        LibraryLine(library: library)
                    }
                    // The licence of both typefaces, once: Fira Sans's file
                    // in full, and the copyright line of the map's labels.
                    LicenceParagraphs(paragraphs: [LicenceParagraph(text: LicenceWords.typeface, isHeading: true)])
                    LicenceParagraphs(paragraphs: Self.texts.typeface + Self.texts.labelsCopyright)
                }
            }
        }
        .navigationTitle(LicenceWords.title)
        .navigationBarTitleDisplayMode(.inline)
    }

    /// The files, read and cut once for the app's run: the core's is long.
    private static let texts = Texts.read()

    struct Texts: Sendable {
        var core: [LicenceParagraph] = []
        var mapLibre: [LicenceParagraph] = []
        var typeface: [LicenceParagraph] = []
        var labelsCopyright: [LicenceParagraph] = []

        static func read(bundle: Bundle = .main) -> Texts {
            func paragraphs(_ name: String, in folder: String? = nil) -> [LicenceParagraph] {
                OpenSourceLicences.text(name, in: folder, bundle: bundle).map(LicenceText.paragraphs) ?? []
            }
            return Texts(
                core: paragraphs(OpenSourceLicences.coreNotices),
                mapLibre: paragraphs(OpenSourceLicences.mapLibreText),
                typeface: paragraphs(OpenSourceLicences.typefaceText),
                labelsCopyright: paragraphs(OpenSourceLicences.typefaceText, in: "MapDemo/fonts")
                    .filter { $0.text.hasPrefix("Copyright") }
            )
        }
    }
}

/// A part's row, which opens and folds it.
private struct LicencePart: View {
    let title: String
    let identifier: String
    @Binding var isOpen: Bool

    var body: some View {
        Button {
            isOpen.toggle()
        } label: {
            HStack(spacing: VentiSpace.s2) {
                Text(title)
                    .font(.venti(.headline))
                    .foregroundStyle(.venti(.text))
                Spacer(minLength: 0)
                Image(systemName: isOpen ? "chevron.up" : "chevron.down")
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.link))
                    .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(.isHeader)
        .accessibilityValue(isOpen ? "Shown" : "Hidden")
        .accessibilityIdentifier(identifier)
    }
}

/// A licence file, a paragraph to a row: the core's is long.
private struct LicenceParagraphs: View {
    let paragraphs: [LicenceParagraph]

    var body: some View {
        ForEach(Array(paragraphs.enumerated()), id: \.offset) { _, paragraph in
            Group {
                if paragraph.isHeading {
                    Text(paragraph.text)
                        .font(.venti(.headline))
                        .foregroundStyle(.venti(.text))
                        .padding(.top, VentiSpace.s3)
                        .accessibilityAddTraits(.isHeader)
                } else {
                    Text(paragraph.text)
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.textSecondary))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .textSelection(.enabled)
            .listRowSeparator(.hidden)
        }
    }
}

/// One library: its name, and under it the version and the licence.
private struct LibraryLine: View {
    let library: OpenSourceLicences.Library

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            Text(library.name)
                .font(.venti(.body))
                .foregroundStyle(.venti(.text))
            Text(library.line)
                .font(.venti(.subheadline))
                .foregroundStyle(.venti(.textSecondary))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, VentiSpace.s1)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("licences-library")
    }
}
