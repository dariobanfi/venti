import SwiftUI
import UIKit
import VentiCore

/// The pictures the map draws flying sites with: a dark disc with a white
/// sign for a take-off, a white disc with a dark sign for a landing, so
/// that the two are told apart at a glance and neither is taken for wind,
/// airspace or a track, which have the colours.
@MainActor
enum SiteSymbols {
    static let side: CGFloat = 26

    /// A picture with the name the map knows it by.
    struct Picture {
        let name: String
        let image: UIImage
    }

    static var all: [Picture] {
        SiteKind.all.map { kind in
            Picture(name: kind.pictureName, image: picture(for: kind))
        }
    }

    static func picture(for kind: SiteKind) -> UIImage {
        let ink = UIColor(hex: 0x1F1F1D)
        // A landing is the light one: where the flight ends.
        let (ground, sign) = kind == .landing ? (UIColor.white, ink) : (ink, UIColor.white)
        return UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
            let disc = UIBezierPath(ovalIn: CGRect(x: 2, y: 2, width: side - 4, height: side - 4))
            let canvas = context.cgContext
            canvas.setShadow(
                offset: CGSize(width: 0, height: 0.5), blur: 1.5,
                color: UIColor.black.withAlphaComponent(0.35).cgColor
            )
            ground.setFill()
            disc.fill()
            canvas.setShadow(offset: .zero, blur: 0, color: nil)
            (kind == .landing ? ink : UIColor.white).setStroke()
            disc.lineWidth = 1.5
            disc.stroke()

            let weight = UIImage.SymbolConfiguration(pointSize: 11, weight: .bold)
            if let glyph = UIImage(systemName: kind.systemImage, withConfiguration: weight)?
                .withTintColor(sign, renderingMode: .alwaysOriginal)
            {
                glyph.draw(at: CGPoint(x: (side - glyph.size.width) / 2, y: (side - glyph.size.height) / 2))
            }
        }
    }
}

/// A site's sign as lists and sheets show it.
struct SiteGlyph: View {
    let kind: SiteKind

    var body: some View {
        Image(systemName: kind.systemImage)
            .font(.system(size: 13, weight: .bold))
            .foregroundStyle(kind == .landing ? Color.primary : Color(.systemBackground))
            .frame(width: 28, height: 28)
            .background(kind == .landing ? Color(.systemBackground) : Color.primary, in: .circle)
            .overlay {
                Circle().strokeBorder(Color.primary, lineWidth: kind == .landing ? 1.5 : 0)
            }
            .accessibilityHidden(true)
    }
}

/// The flying sites around the part of the map in view, as a list: for
/// looking one up by name, and for pilots who use VoiceOver, which cannot
/// reach symbols drawn on a map.
struct SiteList: View {
    @Environment(SitesModel.self) private var sites
    @Environment(\.dismiss) private var dismiss

    @State private var search = ""

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(shown, id: \.id) { site in
                        NavigationLink(value: site) {
                            SiteRow(site: site)
                        }
                        .accessibilityIdentifier("site-row")
                    }
                } footer: {
                    if !sites.sites.isEmpty {
                        Text(SiteScreen.caution + "\n" + sites.credits.map(\.text).joined(separator: "\n"))
                    }
                }
            }
            .overlay {
                if sites.sites.isEmpty {
                    ContentUnavailableView(
                        "No flying sites here",
                        systemImage: "mappin.slash",
                        description: Text(sites.problem ?? "Zoom the map in on an area, and the sites its source knows there are listed.")
                    )
                } else if shown.isEmpty {
                    ContentUnavailableView.search(text: search)
                }
            }
            .searchable(text: $search, prompt: "Site")
            .navigationTitle("Flying sites")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: SiteView.self) { site in
                SiteScreen(site: site)
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    /// Those with a name first, by name; then the rest by what they are.
    private var shown: [SiteView] {
        let wanted = search.trimmingCharacters(in: .whitespaces)
        return sites.sites
            .filter { wanted.isEmpty || $0.name.localizedCaseInsensitiveContains(wanted) }
            .sorted { first, second in
                if first.isNamed != second.isNamed {
                    return first.isNamed
                }
                return first.name.localizedCaseInsensitiveCompare(second.name) == .orderedAscending
            }
    }
}

/// One site in a line.
private struct SiteRow: View {
    let site: SiteView

    var body: some View {
        HStack(spacing: 12) {
            SiteGlyph(kind: site.kind)
            VStack(alignment: .leading, spacing: 2) {
                Text(site.name)
                Text(detail)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var detail: String {
        var parts: [String] = []
        if site.isNamed {
            parts.append(site.kindInWords)
        }
        if let elevation = site.elevationM {
            parts.append("\(Format.metres(elevation)) m")
        }
        if !site.windInWords.isEmpty {
            parts.append("wind \(site.windInWords)")
        }
        return parts.isEmpty ? "No more is known" : parts.joined(separator: " · ")
    }
}

/// One flying site: what its source says about it, what that is worth,
/// and a way to keep it as a waypoint.
struct SiteScreen: View {
    let site: SiteView

    @Environment(SitesModel.self) private var sites
    @Environment(WindModel.self) private var wind
    @Environment(WaypointsModel.self) private var waypoints
    @Environment(\.dismiss) private var dismiss

    @State private var kept: String?

    /// What a site on the map is worth, said wherever sites are listed.
    static let caution = "A site on the map is not permission to fly there. Ask the club or the owner, and look at the place."

    var body: some View {
        List {
            Section {
                LabeledContent("What it is", value: site.kindInWords)
                if let elevation = site.elevationM {
                    LabeledContent("Height", value: "\(Format.metres(elevation)) m")
                }
                LabeledContent("Wind it works in") {
                    if site.wind.isEmpty {
                        Text("Not given")
                    } else {
                        HStack(spacing: 10) {
                            Text(site.windInWords)
                            WindRose(spans: site.wind)
                        }
                    }
                }
            }
            if site.official == true || site.discouraged || site.paragliding == false {
                Section {
                    if site.official == true {
                        Label("Marked as an official site", systemImage: "checkmark.seal")
                    }
                    if site.discouraged {
                        Label("Marked as unsafe or discouraged", systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                    }
                    if site.paragliding == false {
                        Label("Marked as not for paragliders", systemImage: "nosign")
                    }
                }
            }
            if wind.server != nil {
                Section {
                    NavigationLink {
                        ForecastScreen(place: site.position, title: site.name)
                    } label: {
                        Label("Forecast for this place", systemImage: "cloud.sun")
                    }
                    .accessibilityIdentifier("site-forecast")
                }
            }
            Section {
                Button(kept == nil ? "Add to my waypoints" : "In your waypoints", systemImage: kept == nil ? "plus" : "checkmark") {
                    do {
                        try waypoints.add(name: site.name, position: site.position, elevationM: site.elevationM)
                        kept = site.name
                    } catch {
                        kept = nil
                    }
                }
                .disabled(kept != nil)
            } footer: {
                Text("As a waypoint the cockpit can navigate to it, also without a network.")
            }
            Section {
                if let address = site.url, let link = URL(string: address) {
                    Link("See or correct it at the source", destination: link)
                }
            } footer: {
                Text(footnote)
            }
        }
        .navigationTitle(site.name)
        .navigationBarTitleDisplayMode(.inline)
    }

    /// The credit and the notice of the site's source, and what a site on
    /// a map is worth.
    private var footnote: String {
        let credit = sites.credits.first { $0.source == site.source }
        let parts = [Self.caution, credit?.notice, credit?.text]
        return parts.compactMap { $0 }.joined(separator: "\n\n")
    }
}

/// The wind directions a site works in, as wedges on a small compass:
/// north at the top, each wedge where the wind may come from.
struct WindRose: View {
    let spans: [WindSpan]

    var body: some View {
        Canvas { context, size in
            let centre = CGPoint(x: size.width / 2, y: size.height / 2)
            let radius = min(size.width, size.height) / 2 - 1
            let ring = Path(ellipseIn: CGRect(
                x: centre.x - radius, y: centre.y - radius, width: 2 * radius, height: 2 * radius
            ))
            context.stroke(ring, with: .color(.secondary), lineWidth: 1)
            for span in spans {
                // A single direction is drawn as the sixteenth of the
                // compass it names.
                let width = span.toDeg - span.fromDeg
                let (from, to) = width == 0
                    ? (span.fromDeg - 11.25, span.fromDeg + 11.25)
                    : (span.fromDeg, span.toDeg)
                var wedge = Path()
                wedge.move(to: centre)
                // Compass degrees run clockwise from north; the canvas
                // measures from east, and its y grows downwards.
                wedge.addArc(
                    center: centre, radius: radius,
                    startAngle: .degrees(from - 90), endAngle: .degrees(to - 90), clockwise: false
                )
                wedge.closeSubpath()
                context.fill(wedge, with: .color(.primary.opacity(0.75)))
            }
        }
        .frame(width: 30, height: 30)
        .accessibilityHidden(true)
    }
}
