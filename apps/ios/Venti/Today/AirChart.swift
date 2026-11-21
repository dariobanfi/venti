import SwiftUI
import VentiCore

/// How heights are laid out in the picture of a day's air: from a floor
/// under the model's ground and the take-off to a ceiling over the
/// thermals, both on a round figure.
///
/// The picture only places what the core gives it. How high the thermals
/// go, the climb and the winds are the core's (`OutlookDay`, `OutlookHour`).
struct AirScale: Equatable {
    /// The lowest height drawn, metres above sea level.
    let floorM: Double
    /// The highest.
    let ceilingM: Double

    /// The scale for a day at a take-off, or none where the model's ground
    /// is not known: nothing can be placed then.
    init?(day: OutlookDay, takeoffM: Double?) {
        guard let ground = day.groundM else { return nil }
        self.init(
            groundM: ground, takeoffM: takeoffM,
            tops: day.hours.compactMap(\.thermalTopM),
            levels: day.hours.flatMap { $0.winds.map(\.altitudeM) }
        )
    }

    /// The scale for a model's ground, a take-off where its height is
    /// known, how high the thermals of the day's hours go, and the
    /// heights the model gives a wind at.
    init(groundM: Double, takeoffM: Double?, tops: [Double], levels: [Double] = []) {
        let low = min(groundM, takeoffM ?? groundM)
        // Room over the thermals, and a kilometre over the take-off on a
        // day without any. The model's levels are far apart: the first
        // of them above that kilometre is in the picture too, so that
        // there is always a wind from well above the take-off.
        let clear = (takeoffM ?? groundM) + 1000
        let above = levels.filter { $0 >= clear }.min() ?? 0
        let high = max(tops.max() ?? 0, clear, above)
        floorM = (low / 500).rounded(.down) * 500
        ceilingM = max((high / 500).rounded(.up) * 500 + 500, floorM + 1500)
    }

    /// How far down a height lies in a picture this tall, in points.
    func y(_ metres: Double, in height: CGFloat) -> CGFloat {
        height * CGFloat(1 - (metres - floorM) / (ceilingM - floorM))
    }

    /// The heights that get a line: every 500 m, or every 1000 m where
    /// the picture spans more than 3000 m.
    var lines: [Double] {
        let step = ceilingM - floorM > 3000 ? 1000.0 : 500.0
        var lines: [Double] = []
        var line = (floorM / step).rounded(.up) * step
        while line <= ceilingM {
            lines.append(line)
            line += step
        }
        return lines
    }
}

/// The air over a take-off through one day, as a picture: for every hour
/// how high the thermals go, as a column in the colour of the climb to
/// expect with the climb over it; a cloud on the column where cumulus caps
/// them; the wind at the model's levels as arrows; the take-off's height
/// as a line, and the model's own ground as a band at the foot.
///
/// It is drawn, not a chart of a library: the columns stand on a ground
/// that is not zero, and the arrows are winds.
struct AirChart: View {
    let day: OutlookDay
    /// The take-off's height, metres above sea level, where its source
    /// gives one.
    let takeoffM: Double?

    /// The width kept for the heights' figures at the left.
    private static let gutter: CGFloat = 40
    /// The height kept for the hours at the foot.
    private static let foot: CGFloat = 20
    /// The height kept for the climbs at the head.
    private static let head: CGFloat = 22

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            Canvas { context, size in
                guard let scale = AirScale(day: day, takeoffM: takeoffM) else { return }
                draw(in: &context, size: size, scale: scale)
            }
            .frame(height: 280)
            .accessibilityElement()
            .accessibilityLabel("The air through the day")
            .accessibilityValue(spoken)
            .accessibilityIdentifier("air-chart")
            Text(caption)
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
        }
    }

    /// What the picture's marks are. The take-off's line is named only
    /// where the take-off has a height to draw it at.
    private var caption: String {
        let takeoff = takeoffM == nil ? "" : "The dashed line is the take-off. "
        return "How high the thermals go hour by hour, in the colour of the climb to expect, with the climb in m/s over each and a cloud where cumulus caps them. The arrows are the wind at the model's levels, in km/h. \(takeoff)The band at the foot is the model's ground, which is smoother than the terrain. Heights in metres above sea level."
    }

    /// The picture in words, for a screen reader: the day's line.
    private var spoken: String {
        day.thermalLine.isEmpty ? "No estimate of the thermals" : day.thermalLine
    }

    private func draw(in context: inout GraphicsContext, size: CGSize, scale: AirScale) {
        let ink = Color.venti(.text)
        let quiet = Color.venti(.textSecondary)
        let plot = CGRect(
            x: Self.gutter, y: Self.head,
            width: size.width - Self.gutter, height: size.height - Self.head - Self.foot
        )
        guard plot.width > 0, plot.height > 0, !day.hours.isEmpty else { return }
        let y = { (metres: Double) in plot.minY + scale.y(metres, in: plot.height) }

        // The row of climbs over the picture says what its figures are.
        if day.hours.contains(where: { ($0.climbMs ?? 0) > 0 }) {
            context.draw(
                Text("m/s").font(.ventiType(.caption)).foregroundStyle(quiet),
                at: CGPoint(x: plot.minX - 6, y: Self.head / 2), anchor: .trailing
            )
        }

        // The heights: a quiet line and its figure.
        for line in scale.lines {
            var rule = Path()
            rule.move(to: CGPoint(x: plot.minX, y: y(line)))
            rule.addLine(to: CGPoint(x: plot.maxX, y: y(line)))
            context.stroke(rule, with: .color(.venti(.border)), lineWidth: 1)
            context.draw(
                Text(Format.metres(line)).font(.ventiType(.caption)).foregroundStyle(quiet),
                at: CGPoint(x: plot.minX - 6, y: y(line)), anchor: .trailing
            )
        }

        // The model's ground: a band from the floor up to it.
        if let ground = day.groundM {
            let band = CGRect(x: plot.minX, y: y(ground), width: plot.width, height: plot.maxY - y(ground))
            context.fill(Path(band), with: .color(.venti(.surfaceRaised)))
            var edge = Path()
            edge.move(to: CGPoint(x: plot.minX, y: y(ground)))
            edge.addLine(to: CGPoint(x: plot.maxX, y: y(ground)))
            context.stroke(edge, with: .color(.venti(.borderStrong)), lineWidth: 1.5)
        }

        let slot = plot.width / CGFloat(day.hours.count)
        for (index, hour) in day.hours.enumerated() {
            let middle = plot.minX + slot * (CGFloat(index) + 0.5)

            // The hour at the foot.
            context.draw(
                Text(hour.hour).font(.ventiType(.caption)).foregroundStyle(quiet),
                at: CGPoint(x: middle, y: plot.maxY + Self.foot / 2)
            )

            // The thermals: a column from the model's ground to where
            // they end, in the colour of the climb.
            if let top = hour.thermalTopM, let ground = day.groundM, top > ground {
                let width = slot * 0.62
                let column = CGRect(x: middle - width / 2, y: y(top), width: width, height: y(ground) - y(top))
                let climb = hour.climbMs ?? 0
                context.fill(
                    Path(roundedRect: column, cornerSize: CGSize(width: 3, height: 3)),
                    with: .color(ThermalBand(metresPerSecond: climb).color)
                )
                if hour.thermalCloud {
                    // Cumulus: a flat-bottomed cloud standing on the column.
                    let cloud = CGRect(x: middle - slot * 0.42, y: y(top) - 9, width: slot * 0.84, height: 9)
                    let shape = Path(roundedRect: cloud, cornerSize: CGSize(width: 4.5, height: 4.5))
                    context.fill(shape, with: .color(.venti(.surface)))
                    context.stroke(shape, with: .color(quiet), lineWidth: 1.5)
                }
                // The climb in the row over the picture, clear of the
                // cloud and of the wind up there.
                if climb > 0 {
                    context.draw(
                        Text(Format.decimal(climb, places: 1))
                            .font(.ventiType(.caption).weight(.semibold))
                            .foregroundStyle(ink),
                        at: CGPoint(x: middle, y: Self.head / 2)
                    )
                }
            }

            // The wind: at 10 m above the model's ground, and at every
            // level well clear of it. The levels just above the ground
            // would lie on top of one another.
            let lowest = hour.winds.first?.altitudeM
            for wind in hour.winds
            where wind.altitudeM == lowest || wind.altitudeM > (lowest ?? 0) + 300 {
                guard wind.altitudeM <= scale.ceilingM else { continue }
                let at = CGPoint(x: middle, y: y(wind.altitudeM))
                arrow(of: wind.wind, at: at, wide: slot >= 36, in: &context, ink: ink)
            }
        }

        // The take-off: a dashed line across, named at its right end.
        if let takeoffM {
            var line = Path()
            line.move(to: CGPoint(x: plot.minX, y: y(takeoffM)))
            line.addLine(to: CGPoint(x: plot.maxX, y: y(takeoffM)))
            context.stroke(line, with: .color(ink), style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
            context.draw(
                Text("Take-off").font(.ventiType(.caption)).foregroundStyle(ink),
                at: CGPoint(x: plot.maxX - 2, y: y(takeoffM) - 2), anchor: .bottomTrailing
            )
        }
    }

    /// A wind as an arrow that points where the air goes, with its speed
    /// beside it where there is room; a dot where the model gives no
    /// direction.
    private func arrow(
        of wind: ForecastWind, at place: CGPoint, wide: Bool,
        in context: inout GraphicsContext, ink: Color
    ) {
        // Left of the column's middle, so that the speed has the right.
        let centre = CGPoint(x: place.x - (wide ? 8 : 0), y: place.y)
        if let from = wind.fromDeg {
            var turned = context
            turned.translateBy(x: centre.x, y: centre.y)
            turned.rotate(by: .degrees(from + 180))
            var shape = Path()
            shape.move(to: CGPoint(x: 0, y: -7))
            shape.addLine(to: CGPoint(x: 4.5, y: 6))
            shape.addLine(to: CGPoint(x: 0, y: 3))
            shape.addLine(to: CGPoint(x: -4.5, y: 6))
            shape.closeSubpath()
            turned.fill(shape, with: .color(ink))
        } else {
            let dot = Path(ellipseIn: CGRect(x: centre.x - 2.5, y: centre.y - 2.5, width: 5, height: 5))
            context.fill(dot, with: .color(ink))
        }
        if wide {
            context.draw(
                Text(wind.label).font(.ventiType(.caption)).foregroundStyle(ink),
                at: CGPoint(x: centre.x + 8, y: centre.y), anchor: .leading
            )
        }
    }
}
