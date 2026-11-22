import SwiftUI
import VentiCore

/// A small drawing of a flight's shape, so flights can be told apart in a
/// list at a glance. It is decoration: the row beside it says which flight
/// it is.
///
/// `large` is the pilot's own logbook's: a little map of its own, a quiet
/// grid of the theme's border under the track, north up, with the take-off
/// as a hollow dot and the landing as a solid one, as the flight's map has
/// them. The web's logbook draws the same (`TrackThumbnail.svelte`).
struct TrackThumbnail: View {
    let outline: [Coordinate]
    var large = false

    @Environment(\.venti) private var theme

    var body: some View {
        let side = large ? VentiSpace.s8 : VentiSpace.s7 + VentiSpace.s1
        Canvas { context, size in
            // The drawing's proportions are the web's: a square of 52
            // units, the track kept 7 (large) or 8 units from the edge.
            let unit = size.width / 52
            if large {
                drawGrid(in: &context, size: size)
            }
            let area = CGRect(origin: .zero, size: size).insetBy(dx: unit * (large ? 7 : 8), dy: unit * (large ? 7 : 8))
            guard let points = TrackShape.points(outline, in: area) else { return }
            context.stroke(
                TrackShape.path(through: points),
                with: .color(theme.primary),
                style: StrokeStyle(lineWidth: unit * (large ? 1.75 : 2), lineCap: .round, lineJoin: .round)
            )
            if large, let first = points.first, let last = points.last {
                let radius = unit * 2.25
                let ring = { (centre: CGPoint) in
                    Path(ellipseIn: CGRect(x: centre.x - radius, y: centre.y - radius, width: radius * 2, height: radius * 2))
                }
                context.fill(ring(first), with: .color(theme.surface))
                context.stroke(ring(first), with: .color(theme.text), lineWidth: unit)
                context.fill(ring(last), with: .color(theme.text))
            }
        }
        .frame(width: side, height: side)
        .background(large ? theme.bg : theme.surfaceRaised, in: .rect(cornerRadius: VentiRadius.m))
        .overlay {
            if large {
                RoundedRectangle(cornerRadius: VentiRadius.m)
                    .strokeBorder(theme.border, lineWidth: 1)
            }
        }
        .accessibilityHidden(true)
    }

    /// Three lines each way, a quarter apart.
    private func drawGrid(in context: inout GraphicsContext, size: CGSize) {
        var grid = Path()
        for quarter in 1...3 {
            let x = size.width * CGFloat(quarter) / 4
            let y = size.height * CGFloat(quarter) / 4
            grid.move(to: CGPoint(x: x, y: 0))
            grid.addLine(to: CGPoint(x: x, y: size.height))
            grid.move(to: CGPoint(x: 0, y: y))
            grid.addLine(to: CGPoint(x: size.width, y: y))
        }
        context.stroke(grid, with: .color(theme.border), lineWidth: size.width / 104)
    }
}

enum TrackShape {
    /// The outline scaled to fit `area` without distortion, north up.
    /// Returns `nil` for an outline with fewer than two points.
    static func path(_ outline: [Coordinate], in area: CGRect) -> Path? {
        points(outline, in: area).map(path(through:))
    }

    /// The outline's points in `area`, as `path(_:in:)` draws them.
    static func points(_ outline: [Coordinate], in area: CGRect) -> [CGPoint]? {
        guard outline.count >= 2 else { return nil }

        // Degrees of longitude are narrower than degrees of latitude by the
        // cosine of the latitude. Over one flight a single factor is enough.
        let narrowing = cos(outline[0].lat * .pi / 180)
        let xs = outline.map { $0.lon * narrowing }
        let ys = outline.map(\.lat)
        guard let west = xs.min(), let east = xs.max(), let south = ys.min(), let north = ys.max() else {
            return nil
        }

        let scale = min(
            Double(area.width) / max(east - west, 1e-9),
            Double(area.height) / max(north - south, 1e-9)
        )
        let centreX = Double(area.midX) - (west + east) / 2 * scale
        let centreY = Double(area.midY) + (south + north) / 2 * scale

        return outline.indices.map { index in
            CGPoint(x: centreX + xs[index] * scale, y: centreY - ys[index] * scale)
        }
    }

    /// A line through the points, in their order.
    static func path(through points: [CGPoint]) -> Path {
        var path = Path()
        path.addLines(points)
        return path
    }
}
