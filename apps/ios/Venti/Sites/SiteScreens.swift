import SwiftUI
import UIKit
import VentiCore

/// The pictures the map draws flying sites with: the design language's
/// pin, its tip on the site. A dark pin with a cream dot is a place to
/// take off from; the same turned round, cream with a dark dot, a place
/// to land, so that the two are told apart at a glance and neither is
/// taken for wind, airspace or a track, which have the colours. A ring
/// in place of the dot is a training hill or a site of another kind.
@MainActor
enum SiteSymbols {
    /// The head is a disc; the tip, at the bottom edge, is the site.
    static let size = CGSize(width: 28, height: 34)

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
        // The map is light in both appearances: the light theme's pair.
        let dark = UIColor(VentiTheme.light.primary)
        let cream = UIColor(VentiTheme.light.onPrimary)
        let (ground, sign) = kind == .landing ? (cream, dark) : (dark, cream)
        return UIGraphicsImageRenderer(size: size).image { _ in
            let radius: CGFloat = 10
            let centre = CGPoint(x: size.width / 2, y: radius + 3)
            // The head, and from where its sides leave the circle two
            // straight lines to the tip.
            let pin = UIBezierPath()
            pin.addArc(
                withCenter: centre, radius: radius, startAngle: .pi * 5 / 6, endAngle: .pi / 6, clockwise: true
            )
            pin.addLine(to: CGPoint(x: centre.x, y: centre.y + 2 * radius))
            pin.close()
            pin.lineJoinStyle = .round
            pin.lineWidth = 2
            ground.setFill()
            pin.fill()
            sign.setStroke()
            pin.stroke()

            let dot = UIBezierPath(ovalIn: CGRect(x: centre.x - 4, y: centre.y - 4, width: 8, height: 8))
            switch kind {
            case .takeoff, .towing, .landing:
                sign.setFill()
                dot.fill()
            case .training, .other:
                dot.lineWidth = 2
                dot.stroke()
            }
        }
    }
}

/// A site's sign as lists and sheets show it: the pack's icon for what it
/// is, on a disc that is dark for a place to take off from and pale for a
/// place to land.
struct SiteGlyph: View {
    let kind: SiteKind

    var body: some View {
        Image(kind.icon)
            .resizable()
            .frame(width: 16, height: 16)
            .foregroundStyle(.venti(kind == .landing ? .text : .onPrimary))
            .frame(width: 28, height: 28)
            .background(.venti(kind == .landing ? .surface : .primary), in: .circle)
            .overlay {
                Circle().strokeBorder(.venti(.borderStrong), lineWidth: kind == .landing ? 1.5 : 0)
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
            VentiList {
                Section {
                    ForEach(shown, id: \.id) { site in
                        NavigationLink(value: site) {
                            SiteRow(site: site)
                        }
                        .accessibilityIdentifier("site-row")
                    }
                } footer: {
                    if !sites.sites.isEmpty {
                        Text(footnote)
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

    /// What a site on the map is worth, how old the list is, whether it is
    /// all the server's sources have, and whose data it is.
    private var footnote: String {
        var lines = [SiteScreen.caution]
        if let asOf = sites.asOfUnixS {
            lines.append("As of \(Format.dayShort(unixSeconds: asOf)): a site added to the source since then is not here yet.")
        }
        if !sites.isWhole {
            lines.append("The server does not hold every site its sources have for this part of the map.")
        }
        return (lines + sites.credits.map(\.text)).joined(separator: "\n")
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
                    .font(.venti(.footnote))
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
        // A site of which nothing else is known is at least somewhere.
        return parts.isEmpty
            ? Format.place(lat: site.position.lat, lon: site.position.lon)
            : parts.joined(separator: " · ")
    }
}

/// One flying site, as the design language's sheet has it: its name with
/// what the forecast says of it, what it is in a line, and the two things
/// to do from here, plan a route and start a flight. Under that, what its
/// source says about it, what that is worth, and a way to keep it as a
/// waypoint.
struct SiteScreen: View {
    let site: SiteView

    @Environment(SitesModel.self) private var sites
    @Environment(WindModel.self) private var wind
    @Environment(WaypointsModel.self) private var waypoints
    @Environment(CommunityModel.self) private var community
    @Environment(TodayModel.self) private var today
    @Environment(AppNavigation.self) private var navigation
    @Environment(FlySession.self) private var fly

    @State private var kept: String?
    /// What the forecast says about flying from here. Absent for a place
    /// to land, without a server, and until the server has answered.
    @State private var outlook: SiteOutlook?
    @State private var isPlanningRoute = false
    /// The newest public flights that took off here. Empty until the
    /// server has answered, and for a site nobody shared a flight from.
    @State private var flown: [PublicFlight] = []

    /// How far from a site a wind station still says something about it,
    /// metres.
    static let stationsWithinM = 15_000.0

    /// What a site on the map is worth, said wherever sites are listed.
    static let caution = "A site on the map is not permission to fly there. Ask the club or the owner, and look at the place."

    var body: some View {
        VentiList {
            Section {
                head
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
            }
            Section {
                LabeledContent("Where", value: Format.place(lat: site.position.lat, lon: site.position.lon))
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
                            .foregroundStyle(.venti(.danger))
                    }
                    if site.paragliding == false {
                        Label("Marked as not for paragliders", systemImage: "nosign")
                    }
                }
            }
            // The wind measured around the site now, from the stations the
            // map already has, and for each whether it comes from a
            // direction the site works in.
            let near = stationsNear(
                stations: wind.stations, place: site.position, elevationM: site.elevationM, wind: site.wind,
                withinM: Self.stationsWithinM, atMost: 3
            )
            if !near.isEmpty {
                Section {
                    ForEach(near, id: \.station.id) { station in
                        NavigationLink {
                            StationScreen(station: station.station)
                        } label: {
                            StationNearbyRow(near: station, now: wind.now)
                        }
                        .accessibilityIdentifier("site-station")
                    }
                } header: {
                    Text("Wind measured nearby")
                } footer: {
                    Text("A station near a site is not the site: it may stand on a summit, in the valley or behind a ridge. A direction that fits says nothing about the wind's strength or its gusts.")
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
            // What others flew from here and shared with everyone: under
            // what the page is opened for, because it arrives a moment
            // later and must not push that away from under a finger.
            if wind.server != nil, !flown.isEmpty {
                FlownFromHere(flights: flown)
            }
            Section {
                if let address = site.url, let link = URL(string: address) {
                    Link("See or correct it at the source", destination: link)
                }
            } footer: {
                Text(footnote)
            }
        }
        // The name stands at the head of the page; the bar keeps it for
        // the way back from the pages further in, without showing it twice.
        .navigationTitle(site.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Color.clear
                    .frame(width: 1, height: 1)
                    .accessibilityHidden(true)
            }
        }
        // What was flown from a place to take off from; a landing field
        // has no flights that began there.
        .task(id: site.id) {
            guard wind.server != nil, site.kind == .takeoff || site.kind == .towing else { return }
            flown = (try? await community.flights(near: site.position)) ?? []
        }
        // What the forecast says about flying from here.
        .task(id: site.id) {
            guard isForTakingOff else { return }
            outlook = await today.outlook(of: site)
        }
        .fullScreenCover(isPresented: $isPlanningRoute) {
            RouteMapPlanner(from: site.position)
        }
    }

    /// Whether flights begin here: the forecast's verdict and "Start
    /// flight" are for such a place.
    private var isForTakingOff: Bool {
        site.uses.contains(.takeoff) && !site.discouraged
    }

    /// The first day the forecast has flying hours for; the first day it
    /// has at all where none has any.
    private var day: OutlookDay? {
        guard let days = outlook?.days else { return nil }
        return days.first { !$0.hours.isEmpty } ?? days.first
    }

    /// The name with the status, the site in a line, what the forecast
    /// says, and the two buttons.
    private var head: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s3) {
            HStack(alignment: .firstTextBaseline) {
                Text(site.name)
                    .font(.ventiType(.heading))
                    .lineLimit(2)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: VentiSpace.s2)
                if let day {
                    VentiStatus(day)
                        .accessibilityIdentifier("site-status")
                }
            }
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                Text(inALine)
                    .foregroundStyle(.venti(.textSecondary))
                if let day {
                    Text(Self.words(of: day))
                        .foregroundStyle(.venti(.textSecondary))
                        .accessibilityIdentifier("site-outlook")
                    if day.thermals != nil {
                        ThermalLine(day: day)
                            .padding(.top, VentiSpace.s1)
                    }
                }
            }
            HStack(spacing: VentiSpace.s3) {
                Button {
                    isPlanningRoute = true
                } label: {
                    Text("Plan route")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.quiet))
                .accessibilityIdentifier("site-plan-route")
                if isForTakingOff, fly.mode == nil {
                    Button {
                        navigation.startFlight()
                    } label: {
                        Text("Start flight")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(.accent))
                    .accessibilityIdentifier("site-start-flight")
                }
            }
            .padding(.top, VentiSpace.s1)
        }
        .padding(.horizontal, VentiSpace.s1)
    }

    /// "Take-off 1550 m, wind from N to NE"
    private var inALine: String {
        var line = site.kindInWords
        if let elevation = site.elevationM {
            line += " \(Format.metres(elevation)) m"
        }
        if !site.windInWords.isEmpty {
            line += ", wind from \(site.windInWords)"
        }
        return line
    }

    /// "Today: Good from noon. Gusts to 26 km/h after 15:00", with the
    /// day as the core names it: the status at the head is that day's.
    static func words(of day: OutlookDay) -> String {
        day.why.isEmpty ? "\(day.name): \(day.headline)" : "\(day.name): \(day.headline). \(day.why)"
    }

    /// The credit and the notice of the site's source, and what a site on
    /// a map is worth.
    private var footnote: String {
        let credit = sites.credits.first { $0.source == site.source }
        let parts = [Self.caution, credit?.notice, credit?.text]
        return parts.compactMap { $0 }.joined(separator: "\n\n")
    }
}

/// A wind station near a site: its wind as the map draws it, how far and
/// which way it stands from the site and how much higher or lower, and
/// whether its wind comes from a direction the site works in.
struct StationNearbyRow: View {
    let near: StationNearby
    /// The current time by the server's clock.
    let now: Int64

    var body: some View {
        let mark = near.station.mark
        HStack(spacing: 12) {
            WindGlyph(mark: mark)
                .frame(width: 28, height: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(near.station.name)
                    .lineLimit(1)
                Text(Self.whereabouts(near))
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
                if let words = Self.fitWords(near) {
                    Text(words)
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 8)
            if !mark.label.isEmpty {
                Text("\(Text(mark.label).font(.venti(.body, .semibold)))\(Text(" km/h").font(.venti(.footnote)).foregroundStyle(.secondary))")
                    .monospacedDigit()
            } else {
                Text(Self.ageWords(near.station, now: now))
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// "2.1 km NE · 300 m higher"
    static func whereabouts(_ near: StationNearby) -> String {
        var text = "\(Format.kilometres(metres: near.distanceM)) km \(Format.compassPoint(degrees: near.bearingDeg))"
        if let higher = near.higherByM, abs(higher) >= 50 {
            let rounded = (abs(higher) / 10).rounded() * 10
            text += " · \(Format.metres(rounded)) m \(higher > 0 ? "higher" : "lower")"
        }
        return text
    }

    /// Whether the station's wind comes from a direction the site works
    /// in, with the direction; `nil` when that cannot be said.
    static func fitWords(_ near: StationNearby) -> String? {
        guard let fits = near.fits, let from = near.station.latest?.directionDeg else { return nil }
        let point = Format.compassPoint(degrees: from)
        return fits
            ? "From \(point): a direction the site works in"
            : "From \(point): not a direction the site works in"
    }

    /// What stands in place of a wind that is not current.
    static func ageWords(_ station: WindStation, now: Int64) -> String {
        guard let reading = station.latest else { return "no reading" }
        return Format.age(minutes: Format.minutes(from: reading.timeUnixS, to: now))
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
