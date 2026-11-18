import SwiftUI
import VentiCore

/// A small drawing of a flight's shape, so flights can be told apart in a
/// list at a glance.
struct TrackThumbnail: View {
    let outline: [Coordinate]

    var body: some View {
        Canvas { context, size in
            let area = CGRect(origin: .zero, size: size).insetBy(dx: 8, dy: 8)
            guard let path = TrackShape.path(outline, in: area) else { return }
            context.stroke(
                path,
                with: .color(Palette.accent),
                style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round)
            )
        }
        .frame(width: 52, height: 52)
        .background(Color(.tertiarySystemFill), in: .rect(cornerRadius: 12))
        .accessibilityHidden(true)
    }
}

enum TrackShape {
    /// The outline scaled to fit `area` without distortion, north up.
    /// Returns `nil` for an outline with fewer than two points.
    static func path(_ outline: [Coordinate], in area: CGRect) -> Path? {
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

        var path = Path()
        for index in outline.indices {
            let point = CGPoint(x: centreX + xs[index] * scale, y: centreY - ys[index] * scale)
            if index == 0 {
                path.move(to: point)
            } else {
                path.addLine(to: point)
            }
        }
        return path
    }
}
