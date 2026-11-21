import SwiftUI
import VentiCore

/// The pilot's own take-offs: every site they kept, as the core lists
/// them, the nearest first where the iPhone may say where the pilot is and
/// by name where not. A site opens its page; a swipe forgets it.
struct KeptSitesScreen: View {
    @Environment(KeptSitesModel.self) private var own

    /// Where the pilot is, read only if the app may already: this page
    /// asks nobody.
    @State private var position = PilotPosition()

    var body: some View {
        let list = own.list
        VentiList {
            if let list, !list.sites.isEmpty {
                Section {
                    ForEach(list.sites, id: \.site.id) { kept in
                        NavigationLink {
                            SiteScreen(site: kept.site, credits: list.credits, asOfUnixS: kept.asOfUnixS)
                        } label: {
                            KeptSiteRow(kept: kept)
                        }
                        .accessibilityIdentifier("kept-site-row")
                        .swipeActions {
                            Button("Forget", role: .destructive) {
                                Task {
                                    try? await own.forget(kept.site.id)
                                }
                            }
                        }
                    }
                } footer: {
                    Text(Self.footnote(list))
                        .accessibilityIdentifier("kept-sites-note")
                }
            }
        }
        .overlay {
            if let problem = own.problem {
                ContentUnavailableView {
                    Label {
                        Text("Your take-offs could not be read")
                    } icon: {
                        Image("icon-caution")
                    }
                } description: {
                    Text(problem)
                }
            } else if let list, list.sites.isEmpty {
                VentiEmpty(
                    title: "No take-offs of your own yet",
                    text: "Open a site on the map or on Today and keep it as your take-off. Today then lists it first.\n\n\(list.note)"
                ) {}
            }
        }
        .navigationTitle("Your take-offs")
        .navigationBarTitleDisplayMode(.inline)
        // Read when the page comes up, when the position arrives, and
        // after the pilot kept or forgot a site, here or on its page.
        .task(id: Asked(place: position.place, changes: own.changes)) {
            position.readIfAllowed()
            await own.load(from: position.place)
        }
    }

    /// What decides that the core is asked again.
    private struct Asked: Equatable {
        let place: Coordinate?
        let changes: Int
    }

    /// The core's note on where the list is kept, and whose data the
    /// sites are.
    static func footnote(_ list: KeptSites) -> String {
        ([list.note] + list.credits.map(\.text)).joined(separator: "\n")
    }
}

/// One of the pilot's own sites in a line: what it is, how far and which
/// way it lies where a place is known, and the day its record is of.
private struct KeptSiteRow: View {
    let kept: KeptSite

    var body: some View {
        HStack(spacing: VentiSpace.s3) {
            SiteGlyph(kind: kept.site.kind)
            VStack(alignment: .leading, spacing: 2) {
                Text(kept.site.name)
                Text(KeptSitesScreen.detail(of: kept))
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

extension KeptSitesScreen {
    /// "Take-off · 12.4 km NE · as of 18 Nov 2026"
    static func detail(of kept: KeptSite) -> String {
        var parts: [String] = []
        if kept.site.isNamed {
            parts.append(kept.site.kindInWords)
        }
        if let distance = kept.distanceM {
            var away = "\(Format.kilometres(metres: distance)) km"
            if let bearing = kept.bearingDeg {
                away += " \(Format.compassPoint(degrees: bearing))"
            }
            parts.append(away)
        }
        if let asOf = kept.asOfUnixS {
            parts.append("as of \(Format.dayShort(unixSeconds: asOf))")
        }
        // A site of which nothing else is known is at least somewhere.
        return parts.isEmpty
            ? Format.place(lat: kept.site.position.lat, lon: kept.site.position.lon)
            : parts.joined(separator: " · ")
    }
}
