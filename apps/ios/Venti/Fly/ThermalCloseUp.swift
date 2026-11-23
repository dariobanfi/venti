import SwiftUI
import VentiCore

/// Where the things of the thermal view go: metres around a middle, and
/// how many of them the view shows.
///
/// The view only places what the core gives it. Which places, the circle
/// to fly and the words are the core's (`lift`, `trail`, `thermals`); this
/// says how far apart they are drawn.
struct CloseUpGeometry: Equatable {
    /// The middle of the picture.
    let middle: Coordinate
    /// How many metres the picture's width shows.
    let acrossM: Double

    /// The circle is never looked at closer than this.
    static let closestM = 150.0
    /// On the glide: the last minutes of track around the glider.
    static let glideM = 600.0

    /// The picture for what the cockpit shows now, or none before there is
    /// a position.
    init?(view: CockpitView) {
        guard let position = view.position else { return nil }
        guard let lift = view.lift, !lift.points.isEmpty else {
            middle = position
            acrossM = Self.glideM
            return
        }
        // The circles flown, the circle to fly and the glider itself: all
        // in view, with a little air around them.
        var places = lift.points.map { Coordinate(lat: $0.lat, lon: $0.lon) }
        places.append(contentsOf: lift.centre?.outline ?? [])
        places.append(position)
        let (lats, lons) = (places.map(\.lat), places.map(\.lon))
        let centre = Coordinate(
            lat: ((lats.min() ?? position.lat) + (lats.max() ?? position.lat)) / 2,
            lon: ((lons.min() ?? position.lon) + (lons.max() ?? position.lon)) / 2
        )
        middle = centre
        let reach = places.map { place -> Double in
            let offset = Self.metres(from: centre, to: place)
            return max(abs(offset.east), abs(offset.north))
        }.max() ?? 0
        acrossM = max(Self.closestM, reach * 2 * 1.25)
    }

    /// How far east and north of `origin` a place lies, in metres. Good
    /// to a metre over the few hundred metres the view shows.
    static func metres(from origin: Coordinate, to place: Coordinate) -> (east: Double, north: Double) {
        let east = (place.lon - origin.lon) * 111_320 * cos(origin.lat * .pi / 180)
        let north = (place.lat - origin.lat) * 110_540
        return (east, north)
    }

    /// Where a place is drawn in a picture of this size: north up, the
    /// middle in the middle, the width showing `acrossM`.
    func point(_ place: Coordinate, in size: CGSize) -> CGPoint {
        let offset = Self.metres(from: middle, to: place)
        let pointsPerMetre = size.width / acrossM
        return CGPoint(
            x: size.width / 2 + offset.east * pointsPerMetre,
            y: size.height / 2 - offset.north * pointsPerMetre
        )
    }

    /// The length the scale shows: the longest round one that takes no
    /// more than two fifths of the width.
    var scaleM: Double {
        [500, 200, 100, 50, 20, 10].first { Double($0) <= acrossM * 0.4 }.map(Double.init) ?? 10
    }
}

/// The close-up of the air the glider is in, beside the cockpit's map.
///
/// While the glider circles: the places it flew through in the last
/// circles, each where its air is now, coloured by the climb and larger
/// where this thermal climbs better, with the circle to fly next. On the
/// glide, and on the ground: the last minutes of track and the thermals
/// left behind. Always the wind, large: an arrow that points where the
/// air goes and its speed, the core's (`windShown`: measured, a station's
/// or the forecast's) with where it comes from. It is drawn, not a map:
/// north up, no tiles, there without a network.
struct ThermalCloseUp: View {
    let view: CockpitView

    var body: some View {
        VStack(spacing: 0) {
            Canvas { context, size in
                guard let geometry = CloseUpGeometry(view: view) else { return }
                draw(in: &context, size: size, geometry: geometry)
            }
            .overlay(alignment: .topLeading) {
                CloseUpWind(wind: view.windShown)
                    .padding(VentiSpace.s2)
            }
            if let centre = view.lift?.centre {
                HStack(spacing: 6) {
                    Image(systemName: centre.isCentred ? "scope" : "arrow.up")
                        // North up: the arrow points the way to move.
                        .rotationEffect(.degrees(centre.isCentred ? 0 : centre.towardDeg))
                    Text(centre.words)
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.9)
                }
                .font(.venti(.subheadline, .semibold))
                .foregroundStyle(.venti(.text))
                .padding(.horizontal, VentiSpace.s2)
                .frame(maxWidth: .infinity, minHeight: 32)
                .background(.venti(.surface))
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("Lift")
                .accessibilityValue(centre.words)
                .accessibilityIdentifier("lift-badge")
            }
        }
        .background(.venti(.bg))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Thermal view")
        .accessibilityValue(view.lift == nil ? "The track of the last minutes" : "The last circles")
        .accessibilityIdentifier("thermal-view")
    }

    private func draw(in context: inout GraphicsContext, size: CGSize, geometry: CloseUpGeometry) {
        let ink = Color.venti(.text)
        if let lift = view.lift, !lift.points.isEmpty {
            // The places flown through, oldest first, so the newest lie on top.
            for place in lift.points {
                let at = geometry.point(Coordinate(lat: place.lat, lon: place.lon), in: size)
                let radius = 3 + 5 * place.strength
                let dot = Path(ellipseIn: CGRect(x: at.x - radius, y: at.y - radius, width: radius * 2, height: radius * 2))
                context.fill(dot, with: .color(ClimbBand(metresPerSecond: place.climbMs).color))
            }
            if let centre = lift.centre, centre.outline.count >= 2 {
                var ring = Path()
                ring.addLines(centre.outline.map { geometry.point($0, in: size) })
                ring.closeSubpath()
                context.stroke(ring, with: .color(ink), style: StrokeStyle(lineWidth: 2, dash: [5, 4]))
                let middle = geometry.point(centre.position, in: size)
                var cross = Path()
                cross.move(to: CGPoint(x: middle.x - 6, y: middle.y))
                cross.addLine(to: CGPoint(x: middle.x + 6, y: middle.y))
                cross.move(to: CGPoint(x: middle.x, y: middle.y - 6))
                cross.addLine(to: CGPoint(x: middle.x, y: middle.y + 6))
                context.stroke(cross, with: .color(ink), lineWidth: 2)
            }
        } else {
            // The track behind the glider, each stretch in the colour of
            // the climb it was flown in, on a dark casing.
            let places = view.trail.map { (geometry.point(Coordinate(lat: $0.lat, lon: $0.lon), in: size), $0.climbMs) }
            if places.count >= 2 {
                var whole = Path()
                whole.addLines(places.map(\.0))
                context.stroke(whole, with: .color(ink), style: StrokeStyle(lineWidth: 7, lineCap: .round, lineJoin: .round))
                for (from, to) in zip(places, places.dropFirst()) {
                    var stretch = Path()
                    stretch.move(to: from.0)
                    stretch.addLine(to: to.0)
                    context.stroke(
                        stretch, with: .color(ClimbBand(metresPerSecond: to.1).color),
                        style: StrokeStyle(lineWidth: 4, lineCap: .round)
                    )
                }
            }
            // Where the lift was: the thermals left behind, with their climb.
            for thermal in view.thermals {
                let at = geometry.point(thermal.position, in: size)
                guard CGRect(origin: .zero, size: size).insetBy(dx: 8, dy: 8).contains(at) else { continue }
                let mark = Path(ellipseIn: CGRect(x: at.x - 6, y: at.y - 6, width: 12, height: 12))
                context.fill(mark, with: .color(ClimbBand.climb.color))
                context.stroke(mark, with: .color(ink), lineWidth: 2)
                context.draw(
                    Text(thermal.label).font(.venti(.subheadline, .semibold)).foregroundStyle(ink),
                    at: CGPoint(x: at.x, y: at.y - 16)
                )
            }
        }

        // How long a stretch is: a line of a round length, with the length.
        let length = geometry.scaleM / geometry.acrossM * size.width
        let foot = size.height - 14
        var scale = Path()
        scale.move(to: CGPoint(x: 8, y: foot))
        scale.addLine(to: CGPoint(x: 8 + length, y: foot))
        context.stroke(scale, with: .color(ink), lineWidth: 3)
        context.draw(
            Text("\(Int(geometry.scaleM)) m").font(.venti(.subheadline, .semibold)).foregroundStyle(ink),
            at: CGPoint(x: 8 + length + 6, y: foot), anchor: .leading
        )

        // The glider: the own-position arrow, pointing where it flies.
        if let position = view.position {
            let at = geometry.point(position, in: size)
            var glider = context
            glider.translateBy(x: at.x, y: at.y)
            if let course = view.courseDeg {
                glider.rotate(by: .degrees(course))
                var arrow = Path()
                arrow.move(to: CGPoint(x: 0, y: -13))
                arrow.addLine(to: CGPoint(x: 10, y: 11))
                arrow.addLine(to: CGPoint(x: 0, y: 6))
                arrow.addLine(to: CGPoint(x: -10, y: 11))
                arrow.closeSubpath()
                glider.fill(arrow, with: .color(Color(uiColor: Palette.ownPositionOnMap)))
                glider.stroke(
                    arrow, with: .color(Color(uiColor: Palette.ownPositionRingOnMap)),
                    style: StrokeStyle(lineWidth: 3, lineJoin: .round)
                )
            } else {
                let dot = Path(ellipseIn: CGRect(x: -7, y: -7, width: 14, height: 14))
                glider.fill(dot, with: .color(Color(uiColor: Palette.ownPositionOnMap)))
                glider.stroke(dot, with: .color(Color(uiColor: Palette.ownPositionRingOnMap)), lineWidth: 3)
            }
        }
    }
}

/// The wind in the thermal view's corner, large enough to read in flight:
/// an arrow that points where the air goes (north up) and the speed in
/// km/h, with where it comes from under it in the core's words
/// ("measured", "measured 7 min ago", "forecast", a station's name);
/// without any, two dashes.
private struct CloseUpWind: View {
    let wind: CockpitWindView?

    var body: some View {
        HStack(alignment: .center, spacing: VentiSpace.s2) {
            if let wind {
                Image("icon-arrow-wind")
                    .resizable()
                    .frame(width: 32, height: 32)
                    .rotationEffect(.degrees(wind.fromDeg + 180))
            }
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .firstTextBaseline, spacing: VentiSpace.s1) {
                    if let speed = wind?.speedKmh {
                        Text(speed)
                            .font(.ventiType(.instrumentM))
                            .monospacedDigit()
                    } else {
                        NumberPlaceholder(font: .ventiType(.instrumentM), inset: VentiSpace.s2)
                    }
                    Text("km/h")
                        .font(.ventiType(.label))
                }
                if let words = wind?.sourceWords, !words.isEmpty {
                    Text(words)
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.textSecondary))
                }
            }
        }
        .lineLimit(1)
        .foregroundStyle(.venti(.text))
        .padding(.horizontal, VentiSpace.s2)
        .padding(.vertical, VentiSpace.s1)
        .ventiFloating(in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Wind")
        .accessibilityValue(spoken)
        .accessibilityIdentifier("thermal-view-wind")
    }

    private var spoken: String {
        guard let wind else { return "not known yet" }
        let words = "\(Format.spokenKilometresPerHour(metresPerSecond: wind.speedMs)) from \(Format.compassPoint(degrees: wind.fromDeg))"
        return wind.sourceWords.isEmpty ? words : "\(words), \(wind.sourceWords)"
    }
}
